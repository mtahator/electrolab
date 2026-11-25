//! The pictures of a paper (P1.M5.UI.3, UND.5): looked at only with consent, by a model on this
//! computer; what was seen is kept with the paper, piece by piece.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ep_bus::{CommandError, Cx, Recv, Subscription};
use ep_document_contract::{Contents, DocObject, ObjectRef, PageObjects, RegionHit};
use ep_inference_contract::{Picture, Stream, VisionAnswer, VisionRead};
use ep_types::{BlobRef, PageRect};
use ep_understanding_contract::{
    ExplainRegion, FigureSeen, FiguresSeen, FiguresSeenRequest, LookAtFigures, errors, events,
};
use ep_understanding_core::written;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::objects::Target;
use super::prompts::prompts;
use super::{Understanding, failed, head, ms, squash};

/// Most pictures at once.
const MOST: usize = 60;
/// Width of a picture as the model sees it.
const WIDTH: u32 = 900;

/// A figure as it is kept.
#[derive(Clone, Serialize, Deserialize)]
struct Kept {
    rect: PageRect,
    #[serde(flatten)]
    seen: FigureSeen,
}

/// What is kept with a paper.
#[derive(Serialize, Deserialize)]
struct Stored {
    file: String,
    figures: Vec<Kept>,
}

fn same(a: &PageRect, b: &PageRect) -> bool {
    [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() <= 0.5)
}

/// Whether this is the figure that was seen: the same id at the same place.
fn is(k: &Kept, page: u32, o: &DocObject) -> bool {
    k.seen.object.page == page && k.seen.object.id == o.id && same(&k.rect, &o.rect)
}

impl Understanding {
    /// What is kept for this file; nothing when it was seen in another one.
    async fn stored(&self, cx: &Cx, paper: &str, file: &BlobRef) -> Vec<Kept> {
        let all: Vec<Value> = self
            .call(cx, "workspace.get_figures", json!({ "papers": [paper] }))
            .await
            .unwrap_or_default();
        all.into_iter()
            .next()
            .and_then(|v| serde_json::from_value::<Stored>(v["data"].clone()).ok())
            .filter(|s| s.file == file.hash.to_string())
            .map(|s| s.figures)
            .unwrap_or_default()
    }

    /// What was seen in this figure.
    pub(crate) async fn seen(&self, cx: &Cx, i: &ExplainRegion, t: &Target) -> Option<FigureSeen> {
        let (paper, o) = (i.paper.as_ref()?, t.object.as_ref()?);
        let kept = self.stored(cx, paper, &i.file).await;
        kept.into_iter()
            .find(|k| is(k, t.rect.page, o))
            .map(|k| k.seen)
    }

    async fn keep(
        &self,
        cx: &Cx,
        paper: &str,
        file: &BlobRef,
        new: Kept,
    ) -> Result<(), CommandError> {
        let _one = self.keeping.lock().await;
        let mut figures = self.stored(cx, paper, file).await;
        figures.retain(|k| k.seen.object != new.seen.object);
        figures.push(new);
        let data = Stored {
            file: file.hash.to_string(),
            figures,
        };
        self.call::<Value>(
            cx,
            "workspace.save_figures",
            json!({ "paper": paper, "data": data }),
        )
        .await
        .map(|_| ())
    }

    /// Every figure of the paper, in its order.
    async fn figures(&self, cx: &Cx, file: &BlobRef) -> Result<Vec<DocObject>, CommandError> {
        let c: Contents = self
            .call(cx, "document.contents", json!({ "file": file }))
            .await?;
        Ok(c.figures)
    }

    /// `understanding.figures_seen`.
    ///
    /// # Errors
    /// The paper cannot be read.
    pub async fn figures_seen(
        &self,
        cx: &Cx,
        i: FiguresSeenRequest,
    ) -> Result<FiguresSeen, CommandError> {
        let started = Instant::now();
        let all = self.figures(cx, &i.file).await?;
        let kept = self.stored(cx, &i.paper, &i.file).await;
        Ok(FiguresSeen {
            figures: all
                .iter()
                .filter_map(|o| kept.iter().find(|k| is(k, o.rect.page, o)))
                .map(|k| k.seen.clone())
                .collect(),
            total: u32::try_from(all.len()).unwrap_or(u32::MAX),
            model: None,
            total_ms: ms(started),
        })
    }

    /// The figures that are to be looked at.
    async fn wanted(&self, cx: &Cx, i: &LookAtFigures) -> Result<Vec<DocObject>, CommandError> {
        let Some(objects) = &i.objects else {
            let all = self.figures(cx, &i.file).await?;
            let kept = self.stored(cx, &i.paper, &i.file).await;
            return Ok(all
                .into_iter()
                .filter(|o| !kept.iter().any(|k| is(k, o.rect.page, o)))
                .take(MOST)
                .collect());
        };
        let mut out = vec![];
        for r in objects.iter().take(MOST) {
            let here: PageObjects = self
                .call(
                    cx,
                    "document.objects",
                    json!({ "file": i.file, "page": r.page }),
                )
                .await?;
            let o = here.objects.into_iter().find(|o| o.id == r.id);
            out.push(o.ok_or_else(|| failed(errors::NO_OBJECT, "this figure is not there"))?);
        }
        Ok(out)
    }

    /// `understanding.look_at_figures`.
    ///
    /// # Errors
    /// No consent, no such figure, or the errors of `inference.vision_read`.
    pub async fn look_at_figures(
        &self,
        cx: &Cx,
        i: LookAtFigures,
    ) -> Result<FiguresSeen, CommandError> {
        let started = Instant::now();
        if !i.consent {
            return Err(failed(
                "consent_needed",
                "pictures are only looked at when the user agrees",
            ));
        }
        let wanted = self.wanted(cx, &i).await?;
        let mut model = None;
        if !wanted.is_empty() {
            let mut pictures = vec![];
            for o in &wanted {
                let input = json!({ "file": i.file, "page": o.rect.page, "rect": o.rect, "exact": true, "width_px": WIDTH });
                let hit: RegionHit = self.call(cx, "document.region", input).await?;
                let caption = o.text.as_deref().map(squash).filter(|t| !t.is_empty());
                pictures.push(Picture {
                    image: hit.image,
                    question: prompts().picture().to_string(),
                    data: caption.map(|t| format!("Caption: {}", head(&t, 600))),
                });
            }
            model = Some(self.read(cx, &i, &wanted, pictures).await?);
        }
        let mut out = self
            .figures_seen(
                cx,
                FiguresSeenRequest {
                    paper: i.paper,
                    file: i.file,
                },
            )
            .await?;
        out.model = model;
        out.total_ms = ms(started);
        Ok(out)
    }

    /// Have the pictures read; each is kept as soon as it is read. The model that read them.
    async fn read(
        &self,
        cx: &Cx,
        i: &LookAtFigures,
        wanted: &[DocObject],
        pictures: Vec<Picture>,
    ) -> Result<String, CommandError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = format!("figures:{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let models: Value = self.call(cx, "inference.models", Value::Null).await?;
        let model = models["vision"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let one = i.objects.is_some();
        let say = Say {
            id: i.stream.as_ref().map(|s| s.id.as_str()),
            wanted,
        };
        say.looking(self, cx, 0, "");
        let mut sub = self.bus()?.events().subscribe("inference.");
        let request = VisionRead {
            pictures,
            consent: true,
            // All of them is work for later: what a person asks goes first.
            background: !one,
            stream: Some(Stream { id: id.clone() }),
        };
        let request = serde_json::to_value(&request).unwrap_or_default();
        let call = self.call::<VisionAnswer>(cx, "inference.vision_read", request);
        tokio::pin!(call);
        let (mut done, mut text) = (0, String::new());
        let answer = loop {
            tokio::select! {
                // What was said before the end is kept before it ends.
                biased;
                e = sub.recv() => match e {
                    Recv::Event(e) if e.data["id"] == id.as_str() => {
                        if e.kind == "inference.vision" {
                            text.clear();
                            let said = e.data["text"].as_str().unwrap_or_default();
                            self.saw(cx, i, &wanted[done.min(wanted.len() - 1)], said, &model).await?;
                            done += 1;
                            say.looking(self, cx, done, "");
                        } else if one && e.kind == "inference.token" {
                            text.push_str(e.data["text"].as_str().unwrap_or_default());
                            say.looking(self, cx, done, &written::written_so_far(&text));
                        }
                    }
                    Recv::Event(_) | Recv::Missed(_) => {}
                    Recv::Closed => break (&mut call).await,
                },
                r = &mut call => break r,
            }
        };
        match answer {
            Ok(a) => {
                for (o, said) in wanted.iter().zip(&a.texts).skip(done) {
                    self.saw(cx, i, o, said, &a.model).await?;
                    done += 1;
                    say.looking(self, cx, done, "");
                }
                Ok(a.model)
            }
            Err(e) => {
                self.rest(cx, i, &mut sub, &id, &wanted[done..], &model)
                    .await;
                Err(e)
            }
        }
    }

    /// Keep what was read just before the end.
    async fn rest(
        &self,
        cx: &Cx,
        i: &LookAtFigures,
        sub: &mut Subscription,
        id: &str,
        wanted: &[DocObject],
        model: &str,
    ) {
        // The call may have been cancelled; keeping is not.
        let own = Cx::new(
            cx.actor.clone(),
            cx.correlation,
            cx.clock.clone(),
            cx.ids.clone(),
        );
        let mut left = wanted.iter();
        while let Ok(Recv::Event(e)) =
            tokio::time::timeout(std::time::Duration::from_millis(20), sub.recv()).await
        {
            if e.kind == "inference.vision"
                && e.data["id"] == id
                && let Some(o) = left.next()
            {
                let said = e.data["text"].as_str().unwrap_or_default();
                let _ = self.saw(&own, i, o, said, model).await;
            }
        }
    }

    async fn saw(
        &self,
        cx: &Cx,
        i: &LookAtFigures,
        o: &DocObject,
        text: &str,
        model: &str,
    ) -> Result<(), CommandError> {
        let text = squash(&written::written(text));
        if text.is_empty() {
            return Ok(());
        }
        let kept = Kept {
            rect: o.rect,
            seen: FigureSeen {
                object: ObjectRef {
                    page: o.rect.page,
                    id: o.id.clone(),
                },
                label: o.label.clone(),
                text,
                model: model.to_string(),
                at_ms: u64::try_from(cx.clock.now_ms()).unwrap_or(0),
            },
        };
        self.keep(cx, &i.paper, &i.file, kept).await
    }
}

/// Progress of the looking.
struct Say<'a> {
    id: Option<&'a str>,
    wanted: &'a [DocObject],
}

impl Say<'_> {
    fn looking(&self, u: &Understanding, cx: &Cx, done: usize, text: &str) {
        if let (Some(id), Ok(bus)) = (self.id, u.bus()) {
            let label = self.wanted.get(done).and_then(|o| o.label.clone());
            bus.events().publish(
                events::LOOKING,
                json!({ "id": id, "done": done, "total": self.wanted.len(), "label": label, "text": text }),
                cx.clock.now_ms(),
            );
        }
    }
}
