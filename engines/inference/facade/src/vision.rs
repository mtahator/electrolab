//! Looking at pictures (contract: *Looking at pictures*): only with consent, only on this computer,
//! one model at a time, and a person's question goes first.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use ep_bus::{CommandError, Cx};
use ep_inference_adapter_llamacpp::{Config, LlamaCppSidecar};
use ep_inference_contract::{
    Backend, LlmRequest, Message, Picture, RunState, Status, VisionAnswer, VisionModel, VisionRead,
    errors, events,
};
use ep_inference_core::{BackendError, Image, InferenceBackend, prepare};
use ep_types::{BlobRef, Hash};
use serde_json::json;

use crate::catalog::{self, Catalog, Found};
use crate::{Inference, failed, from_backend, select};

/// Where the pictures are: the host knows.
pub type Pictures = Arc<dyn Fn(&Hash) -> Option<PathBuf> + Send + Sync>;

/// Most pictures of one call, and bytes of one picture.
const PICTURES: usize = 60;
const BYTES: u64 = 8 << 20;
/// Context and longest answer.
const CONTEXT: u32 = 4096;
const ANSWER: u32 = 260;
/// Memory the picture part needs while it works, besides its weights.
const WORKING: u64 = 420 << 20;
/// The job in routes and recordings.
const TASK: &str = "vision.read";
const SYSTEM: &str = "You look at a picture from a research paper and answer the question about it \
in plain English. Say only what can be seen in the picture. Read numbers and labels exactly as they \
are printed; when something cannot be read, say so. Do not guess. \nWrite three to six short sentences, without headings or lists.";

/// Counts a batch that is under way; other background work waits for its end.
struct Looking<'a>(&'a Inference);
impl Drop for Looking<'_> {
    fn drop(&mut self) {
        self.0.looking.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How the model is started: the first way that works is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Way {
    /// Layers on the graphics card (missing: llama.cpp decides).
    layers: Option<u32>,
    part_on_cpu: bool,
}

/// The ways to start `model` with `part`, best first. `free` is the card's free memory, when known.
pub(crate) fn ways(model: &Found, part: &Found, backend: Backend, free: Option<u64>) -> Vec<Way> {
    let way = |layers, part_on_cpu| Way {
        layers,
        part_on_cpu,
    };
    if backend == Backend::Cpu {
        return vec![way(Some(0), true)];
    }
    let eye = part.size + WORKING;
    let words = select::need(model, CONTEXT);
    let mut all = Vec::new();
    if free.is_none_or(|f| f >= eye + words) {
        all.push(way(Some(99), false));
    }
    // The picture part is the slow one: it stays on the card as long as it fits there.
    if free.is_none_or(|f| f >= eye) {
        all.push(way(None, false));
    }
    all.push(way(None, true));
    all
}

impl Inference {
    /// Where pictures are read from (the host's store).
    pub fn use_pictures(&self, pictures: Pictures) {
        *self.pictures.write() = Some(pictures);
    }

    /// The model that looks at pictures.
    pub(crate) fn vision_model(&self, c: &Catalog) -> Option<VisionModel> {
        if let Some(b) = &self.fixed {
            let id = b.model().unwrap_or_default();
            return Some(VisionModel {
                name: id.clone(),
                id,
            });
        }
        catalog::vision(&c.models).map(|(m, _)| VisionModel {
            id: m.id.clone(),
            name: m.name.clone(),
        })
    }

    fn picture(&self, r: &BlobRef) -> Result<Image, CommandError> {
        if r.size > BYTES {
            return Err(CommandError::InvalidInput {
                message: "the picture is too large".into(),
            });
        }
        let find = self.pictures.read().clone();
        let bytes = find
            .and_then(|f| f(&r.hash))
            .and_then(|p| std::fs::read(p).ok())
            .ok_or_else(|| failed(errors::IO, "the picture could not be read"))?;
        Ok(Image {
            media_type: r.media_type.0.clone(),
            bytes: Arc::new(bytes),
        })
    }

    /// The running model that looks at pictures, started when needed (the caller has the turn).
    async fn eyes(self: &Arc<Self>, cx: &Cx) -> Result<Arc<dyn InferenceBackend>, CommandError> {
        if let Some(b) = &self.fixed {
            return Ok(b.clone());
        }
        let c = self.searched(false).await;
        let (model, part) = catalog::vision(&c.models).ok_or_else(|| {
            failed(
                errors::NO_VISION_MODEL,
                "no model that looks at pictures was found",
            )
        })?;
        let running = self.sidecar.read().clone();
        if let Some(s) = running
            && s.config().mmproj.as_ref() == Some(&part.path)
            && s.is_running().await
        {
            return Ok(s);
        }
        let _one = self.starting.lock().await;
        let (backend, exe) = self.runtime();
        let exe = exe.ok_or_else(|| failed(errors::NO_RUNTIME, "no llama.cpp build found"))?;
        let late = self.sidecar.read().is_some();
        self.stop_current(cx).await;
        let status = |state, error, ready_ms| Status {
            state,
            model: Some(model.id.clone()),
            backend: Some(backend),
            error,
            wanted: None,
            fallback: None,
            ready_ms,
        };
        *self.status.lock() = status(RunState::Starting, None, None);
        self.publish(events::MODEL_LOADING, json!({ "model": model.id }), cx);
        let measured = if backend == Backend::Cpu {
            None
        } else {
            let enough = part.size + WORKING + select::need(model, CONTEXT);
            select::settled(
                || self.measure(&exe),
                |(m, _)| !late || m.free >= enough.min(m.total * 3 / 4),
            )
            .await
        };
        let spawner = self
            .spawner
            .clone()
            .ok_or_else(|| failed(errors::NO_RUNTIME, "no process starter"))?;
        let mut last = BackendError::StartFailed("not started".into());
        for way in ways(model, part, backend, measured.as_ref().map(|(m, _)| m.free)) {
            let sidecar = Arc::new(LlamaCppSidecar::new(
                Config {
                    exe: exe.clone(),
                    model: model.path.clone(),
                    mmproj: Some(part.path.clone()),
                    mmproj_on_cpu: way.part_on_cpu,
                    model_id: model.id.clone(),
                    context: CONTEXT,
                    gpu_layers: way.layers,
                    device: measured.as_ref().map(|(_, d)| d.clone()),
                    compact: true,
                    backend: format!("{backend:?}").to_lowercase(),
                    log_file: self.log_file.clone(),
                },
                spawner.clone(),
            ));
            match sidecar.start().await {
                Ok(took) => {
                    let ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
                    *self.sidecar.write() = Some(sidecar.clone());
                    *self.status.lock() = status(RunState::Ready, None, Some(ms));
                    self.publish(
                        events::MODEL_LOADED,
                        json!({ "model": model.id, "backend": backend, "ms": ms }),
                        cx,
                    );
                    return Ok(sidecar);
                }
                Err(e) => {
                    tracing::warn!(model = %model.id, ?way, %e, "the vision model did not start this way");
                    last = e;
                }
            }
        }
        let err = from_backend(last);
        *self.status.lock() = status(RunState::Failed, Some(err.to_string()), None);
        Err(err)
    }

    /// Read pictures, one after the other.
    ///
    /// # Errors
    /// No consent, no model, or it could not be used. What was read before is in the events.
    pub async fn vision_read(
        self: &Arc<Self>,
        cx: &Cx,
        i: VisionRead,
    ) -> Result<VisionAnswer, CommandError> {
        if !i.consent {
            return Err(failed(
                errors::CONSENT_NEEDED,
                "pictures are only read when the user agrees",
            ));
        }
        if i.pictures.is_empty() || i.pictures.len() > PICTURES {
            return Err(CommandError::InvalidInput {
                message: format!("between 1 and {PICTURES} pictures"),
            });
        }
        let images = i
            .pictures
            .iter()
            .map(|p| self.picture(&p.image))
            .collect::<Result<Vec<_>, _>>()?;
        let started = Instant::now();
        let total = images.len();
        self.looking.fetch_add(1, Ordering::SeqCst);
        let looking = Looking(self);
        let mut texts: Vec<String> = Vec::with_capacity(total);
        let mut model = String::new();
        let mut broken = None;
        for (p, image) in i.pictures.iter().zip(images) {
            match self.look(cx, &i, p, image).await {
                Ok((text, by)) => {
                    model = by;
                    if let Some(s) = &i.stream {
                        self.publish(
                            events::VISION,
                            json!({ "id": s.id, "done": texts.len() + 1, "total": total, "text": text }),
                            cx,
                        );
                    }
                    texts.push(text);
                }
                Err(e) => {
                    broken = Some(e);
                    break;
                }
            }
        }
        drop(looking);
        self.give_back(cx);
        match broken {
            Some(e) => Err(e),
            None => Ok(VisionAnswer {
                texts,
                model,
                total_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            }),
        }
    }

    /// Read one picture; when a person asks in the middle, it is begun again afterwards.
    async fn look(
        self: &Arc<Self>,
        cx: &Cx,
        all: &VisionRead,
        p: &Picture,
        image: Image,
    ) -> Result<(String, String), CommandError> {
        let mut req = LlmRequest::new(
            TASK,
            vec![Message::system(SYSTEM), Message::user(p.question.clone())],
        );
        req.data.clone_from(&p.data);
        req.limits.max_tokens = ANSWER;
        let mut prepared = prepare(&req);
        prepared.images = vec![image];
        loop {
            let turn = self.take_turn(cx, all.background).await?;
            let backend = self.eyes(cx).await?;
            if !backend.is_local() {
                return Err(failed(
                    errors::REMOTE_NOT_ALLOWED,
                    "pictures may only go to a model on this computer",
                ));
            }
            let model = backend.model().unwrap_or_default();
            self.publish(
                events::ROUTE_DECIDED,
                json!({ "task": TASK, "backend": backend.name(), "model": model, "reason": "the model that looks at pictures; local-only data" }),
                cx,
            );
            let on_token = |t: &str| {
                if let Some(s) = &all.stream {
                    self.publish(events::TOKEN, json!({ "id": s.id, "text": t }), cx);
                }
            };
            let cancelled =
                || cx.is_cancelled() || (all.background && self.people.load(Ordering::SeqCst) > 0);
            let r = backend.complete(&prepared, &on_token, &cancelled).await;
            drop(turn);
            match r {
                Ok(c) => return Ok((c.text.trim().to_string(), model)),
                Err(BackendError::Cancelled) if !cx.is_cancelled() => {}
                Err(e) => return Err(from_backend(e)),
            }
        }
    }

    /// Afterwards the answering model is made ready again, or the memory is given back.
    fn give_back(self: &Arc<Self>, cx: &Cx) {
        if self.fixed.is_some() {
            return;
        }
        // The call may have been cancelled; this is its own work.
        let own = Cx::new(
            cx.actor.clone(),
            cx.correlation,
            cx.clock.clone(),
            cx.ids.clone(),
        );
        let (me, cx) = (self.clone(), own);
        tokio::spawn(async move {
            let Ok(_turn) = me.take_turn(&cx, true).await else {
                return;
            };
            let running = me.sidecar.read().clone();
            if running.is_none_or(|s| s.config().mmproj.is_none())
                || me.looking.load(Ordering::SeqCst) > 0
            {
                return;
            }
            if me.choices.read().on_demand {
                me.unload(&cx).await;
            } else if let Err(e) = me.load(&cx, None).await {
                tracing::info!(%e, "no model was made ready after looking at pictures");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ep_inference_contract::ModelKind;

    fn found(mib: u64) -> Found {
        Found {
            id: "m".into(),
            name: "m".into(),
            path: PathBuf::from("m"),
            size: mib << 20,
            licence: String::new(),
            kind: ModelKind::Chat,
            architecture: "x".into(),
            quantisation: None,
            context: 8192,
            cache_per_token: Some(50_000),
        }
    }

    #[test]
    fn the_picture_part_keeps_its_place_on_the_card_longest() {
        let (m, p) = (found(1800), found(800));
        let on_cpu = |b, free| {
            ways(&m, &p, b, free)
                .iter()
                .map(|w| (w.layers, w.part_on_cpu))
                .collect::<Vec<_>>()
        };
        assert_eq!(on_cpu(Backend::Cpu, None), [(Some(0), true)]);
        let mib = |n: u64| Some(n << 20);
        assert_eq!(
            on_cpu(Backend::Cuda, mib(3900)),
            [(Some(99), false), (None, false), (None, true)]
        );
        assert_eq!(
            on_cpu(Backend::Cuda, mib(2500)),
            [(None, false), (None, true)]
        );
        assert_eq!(on_cpu(Backend::Cuda, mib(900)), [(None, true)]);
    }
}
