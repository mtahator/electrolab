//! A streamed draft and its check: the way every explanation is made (spike S3).

use ep_bus::{CommandError, Cx};
use ep_inference_contract::{LlmRequest, LlmResponse, Message, Stream};
use ep_understanding_contract::{ExplainStatus, events};
use ep_understanding_core::{numbers, written};
use serde_json::json;

use super::Understanding;

/// One question for a draft.
pub(crate) struct Draft<'a> {
    /// Task of the inference engine.
    pub(crate) task: &'a str,
    /// Id of the answer, when it is streamed.
    pub(crate) id: Option<&'a str>,
    pub(crate) system: String,
    pub(crate) question: String,
    /// Words of the paper: data, never instructions (guarantee 1).
    pub(crate) data: String,
    /// How the answer begins.
    pub(crate) prefill: Option<String>,
    pub(crate) tokens: u32,
}

/// Whether the check said the draft is right ("OK" first, as the prompt asks).
fn says_ok(s: &str) -> bool {
    let t = s.trim_start().trim_start_matches(['"', '\'', '*']);
    t.len() >= 2
        && t.is_char_boundary(2)
        && t[..2].eq_ignore_ascii_case("ok")
        && !t[2..].starts_with(|c: char| c.is_alphanumeric())
}

/// The corrected explanation in what the check said; none when it talks about the draft instead.
fn correction(said: &str) -> Option<&str> {
    let text = match said.split_once(':') {
        Some((first, rest)) if first.len() < 60 && first.to_lowercase().contains("explanation") => {
            rest.trim()
        }
        _ => said,
    };
    let lower = text.to_lowercase();
    let about_the_draft = [
        "the explanation",
        "this explanation",
        "the original explanation",
        "the draft",
        "the original text",
        "incorrectly states",
        "is incorrect",
        "is wrong",
        "the correct ",
        "the caption confirms",
    ]
    .iter()
    .any(|w| lower.contains(w));
    (text.split_whitespace().count() >= 8 && !about_the_draft).then_some(text)
}

/// A task that ends with its owner.
struct Forward(tokio::task::JoinHandle<()>);

impl Drop for Forward {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Understanding {
    /// Ask for a draft; it streams as `understanding.explaining`.
    pub(crate) async fn draft(&self, cx: &Cx, d: Draft<'_>) -> Result<LlmResponse, CommandError> {
        let draft_id = d.id.map(|s| format!("{s}:draft"));
        let _forward = draft_id.as_ref().and_then(|did| {
            let mut sub = self.bus().ok()?.events().subscribe("inference.token");
            let (did, id) = (did.clone(), d.id.unwrap_or_default().to_string());
            let mut text = d.prefill.clone().unwrap_or_default();
            let bus = self.bus.read().clone();
            let clock = cx.clock.clone();
            Some(Forward(tokio::spawn(async move {
                while let ep_bus::Recv::Event(e) = sub.recv().await {
                    if e.data["id"] != did.as_str() {
                        continue;
                    }
                    text.push_str(e.data["text"].as_str().unwrap_or_default());
                    if let Some(bus) = bus.upgrade() {
                        bus.events().publish(
                            events::EXPLAINING,
                            json!({ "id": id, "phase": "draft", "text": written::written_so_far(&text) }),
                            clock.now_ms(),
                        );
                    }
                }
            })))
        });
        let mut req = LlmRequest::new(
            d.task,
            vec![Message::system(d.system), Message::user(d.question)],
        );
        req.data = Some(d.data);
        req.prefill = d.prefill;
        req.limits.max_tokens = d.tokens;
        req.stream = draft_id.map(|id| Stream { id });
        let mut said: LlmResponse = self
            .call(
                cx,
                "inference.complete",
                serde_json::to_value(&req).unwrap_or_default(),
            )
            .await?;
        said.content = written::written(&said.content);
        Ok(said)
    }

    /// Check a draft: whether the model agreed, and the text if it corrected it.
    pub(crate) async fn check(
        &self,
        cx: &Cx,
        task: &str,
        system: String,
        question: String,
        data: String,
        tokens: u32,
    ) -> Result<(ExplainStatus, Option<String>), CommandError> {
        let mut req = LlmRequest::new(task, vec![Message::system(system), Message::user(question)]);
        req.data = Some(data);
        req.limits.max_tokens = tokens;
        let said: LlmResponse = self
            .call(
                cx,
                "inference.complete",
                serde_json::to_value(&req).unwrap_or_default(),
            )
            .await?;
        let said = written::written(said.content.trim());
        let said = said.trim();
        Ok(if says_ok(said) {
            (ExplainStatus::Checked, None)
        } else {
            match correction(said) {
                Some(text) => (ExplainStatus::Corrected, Some(numbers::whole(text))),
                None => (ExplainStatus::Unchecked, None),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_correction_is_the_explanation_itself() {
        let good = "Figure 2 shows the error of both models on the test set.";
        assert_eq!(correction(good), Some(good));
        assert_eq!(
            correction(&format!("Corrected explanation: {good}")),
            Some(good)
        );
        assert_eq!(
            correction("The explanation incorrectly states that the error falls in both cases."),
            None,
            "words about the draft are no explanation"
        );
        assert_eq!(correction("It is wrong."), None);
    }

    #[test]
    fn ok_is_read_strictly() {
        assert!(says_ok("OK"));
        assert!(says_ok("OK."));
        assert!(says_ok("\"OK\""));
        assert!(!says_ok("Okay, but PINN means"));
        assert!(!says_ok("PINN stands for … OK."));
        assert!(!says_ok("…"));
    }
}
