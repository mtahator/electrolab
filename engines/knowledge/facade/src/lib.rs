//! Knowledge engine facade: digests of papers, made in the background and kept with them, and asking
//! the library (`engines/knowledge/CONTRACT.md`). It calls Document, Inference and Workspace over the
//! bus only.

mod digest;
mod find;

use std::collections::HashSet;
use std::sync::{Arc, Weak};

use ep_bus::{Bus, CommandError, Cx, Envelope, Permission, RegisterError, Spec, Stability};
pub use ep_knowledge_contract as contract;
use ep_knowledge_contract::{
    DIGEST_VERSION, Digest, DigestInfo, DigestLibrary, DigestRequest, DigestStatus, FindRequest,
    Level, PaperId, Papers, commands as cmd, events,
};
use ep_workspace_contract::{Paper, PaperDigest};
use parking_lot::{Mutex, RwLock};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use digest::Item;

/// The engine.
#[derive(Default)]
pub struct Knowledge {
    bus: RwLock<Weak<Bus>>,
    status: Mutex<DigestStatus>,
    stop: Mutex<Option<CancellationToken>>,
    /// Papers whose digest is made anew when their turn comes.
    anew: Mutex<HashSet<String>>,
}

pub(crate) fn failed(kind: &str, message: impl Into<String>) -> CommandError {
    CommandError::Failed {
        kind: kind.into(),
        message: message.into(),
    }
}

impl Knowledge {
    /// A new engine.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn bus(&self) -> Result<Arc<Bus>, CommandError> {
        self.bus
            .read()
            .upgrade()
            .ok_or_else(|| failed("io", "the bus is gone"))
    }

    /// Call another engine's command; cancelling `cx` cancels it too.
    pub(crate) async fn call<T: DeserializeOwned>(
        &self,
        cx: &Cx,
        command: &str,
        input: Value,
    ) -> Result<T, CommandError> {
        let bus = self.bus()?;
        let env = Envelope::new(command, input);
        let id = env.id;
        let v = tokio::select! {
            r = bus.call(env) => r?,
            () = cx.cancel.cancelled() => {
                bus.cancel(id);
                return Err(CommandError::Cancelled);
            }
        };
        serde_json::from_value(v).map_err(|e| failed("bad_output", format!("{command}: {e}")))
    }

    /// Papers of the library.
    pub(crate) async fn library(
        &self,
        cx: &Cx,
        only: Option<&[PaperId]>,
    ) -> Result<Vec<Item>, CommandError> {
        let all: Vec<Paper> = self
            .call(
                cx,
                "workspace.list_papers",
                json!({ "limit": 10_000, "offset": 0, "sort": "opened" }),
            )
            .await?;
        Ok(all
            .into_iter()
            .filter(|p| only.is_none_or(|o| o.contains(&p.id)))
            .map(|p| Item {
                id: p.id.to_string(),
                title: p.title.unwrap_or(p.original_name),
                file: p.file,
            })
            .collect())
    }

    /// Digests kept in the library (old formats are left out).
    pub(crate) async fn stored(
        &self,
        cx: &Cx,
        only: Option<&str>,
    ) -> Result<Vec<Digest>, CommandError> {
        let papers = only.map(|p| vec![p]);
        let v: Vec<PaperDigest> = self
            .call(cx, "workspace.get_digests", json!({ "papers": papers }))
            .await?;
        Ok(v.into_iter()
            .filter_map(|d| serde_json::from_value::<Digest>(d.data).ok())
            .filter(|d| d.version == DIGEST_VERSION)
            .collect())
    }

    async fn item(&self, cx: &Cx, id: PaperId) -> Result<Item, CommandError> {
        let p: Paper = self
            .call(cx, "workspace.get_paper", json!({ "id": id }))
            .await?;
        Ok(Item {
            id: p.id.to_string(),
            title: p.title.unwrap_or(p.original_name),
            file: p.file,
        })
    }

    /// Make (or finish) the digest of a paper.
    ///
    /// # Errors
    /// The paper has no text, or an engine failed.
    pub async fn digest_paper(&self, cx: &Cx, i: DigestRequest) -> Result<Digest, CommandError> {
        let item = self.item(cx, i.paper).await?;
        let old = self
            .stored(cx, Some(&item.id))
            .await?
            .pop()
            .filter(|_| !i.force);
        if let Some(d) = &old
            && d.level == Level::Full
            && d.complete
        {
            return Ok(d.clone());
        }
        let d = self.full(cx, &item, old, &|_, _| {}).await?;
        self.say_digested(cx, &d);
        Ok(d)
    }

    pub(crate) fn say_digested(&self, cx: &Cx, d: &Digest) {
        if let Ok(bus) = self.bus() {
            bus.events().publish(
                events::DIGESTED,
                json!({ "paper": d.paper, "level": d.level }),
                cx.clock.now_ms(),
            );
        }
    }

    fn set(&self, cx: &Cx, f: impl FnOnce(&mut DigestStatus)) {
        let s = {
            let mut s = self.status.lock();
            f(&mut s);
            s.clone()
        };
        if let Ok(bus) = self.bus() {
            bus.events()
                .publish(events::DIGESTING, json!(s), cx.clock.now_ms());
        }
    }

    /// Start making digests in the background; papers that are named come next.
    fn start(self: &Arc<Self>, cx: &Cx, i: &DigestLibrary) -> DigestStatus {
        let named: Vec<String> = i.papers.iter().flatten().map(ToString::to_string).collect();
        let full = i.full;
        {
            let mut stop = self.stop.lock();
            let mut status = self.status.lock();
            for id in &named {
                if i.force {
                    self.anew.lock().insert(id.clone());
                }
                if !status.next.contains(id) && status.paper.as_ref() != Some(id) {
                    status.next.push(id.clone());
                }
            }
            if status.running {
                status.full |= full;
                return status.clone();
            }
            *stop = Some(CancellationToken::new());
            *status = DigestStatus {
                running: true,
                full,
                next: std::mem::take(&mut status.next),
                ..DigestStatus::default()
            };
        }
        let me = self.clone();
        let mut own = Cx::new(
            cx.actor.clone(),
            cx.correlation,
            cx.clock.clone(),
            cx.ids.clone(),
        );
        own.cancel = self.stop.lock().clone().unwrap_or_default();
        tokio::spawn(async move {
            let r = me.make_all(&own).await;
            me.set(&own, |s| {
                s.running = false;
                s.full = false;
                s.paper = None;
                s.next.clear();
                s.current = None;
                s.step = None;
                s.error = match r {
                    Ok(()) | Err(CommandError::Cancelled) => None,
                    Err(e) => Some(e.to_string()),
                };
            });
        });
        self.status.lock().clone()
    }

    async fn make_all(&self, cx: &Cx) -> Result<(), CommandError> {
        let items = self.library(cx, None).await?;
        let have = self.stored(cx, None).await?;
        let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
        self.set(cx, |s| s.total = total);
        // First the quick ones, so that everything can be found.
        for p in items
            .iter()
            .filter(|p| !have.iter().any(|d| d.paper == p.id))
        {
            self.set(cx, |s| s.current = Some(p.title.clone()));
            match self.basic(cx, p).await {
                Ok((d, _)) => {
                    self.keep(cx, &d).await?;
                    self.say_digested(cx, &d);
                }
                Err(CommandError::Cancelled) => return Err(CommandError::Cancelled),
                // A scanned or damaged paper does not hold up the others.
                Err(e) => tracing::info!(paper = %p.title, %e, "no basic digest"),
            }
            tokio::task::yield_now().await;
        }
        let mut row: std::collections::VecDeque<&Item> = items.iter().collect();
        let mut wrong = 0;
        loop {
            if cx.is_cancelled() {
                return Err(CommandError::Cancelled);
            }
            // Those asked for, then (when all are wanted) the others in order.
            let (asked, all) = {
                let mut s = self.status.lock();
                let asked = (!s.next.is_empty()).then(|| s.next.remove(0));
                (asked, s.full)
            };
            let was_asked = asked.is_some();
            let p = match asked {
                Some(id) => items.iter().find(|p| p.id == id),
                None if all => match row.pop_front() {
                    Some(p) => Some(p),
                    None => break,
                },
                None => break,
            };
            let Some(p) = p else { continue };
            let anew = self.anew.lock().remove(&p.id);
            let old = self.stored(cx, Some(&p.id)).await?.pop().filter(|_| !anew);
            if old
                .as_ref()
                .is_some_and(|d| d.level == Level::Full && d.complete)
            {
                self.set(cx, |s| s.done += u32::from(!was_asked));
                continue;
            }
            self.set(cx, |s| {
                s.current = Some(p.title.clone());
                s.paper = Some(p.id.clone());
            });
            let step = |n: usize, of: usize| self.set(cx, |s| s.step = Some(format!("{n} / {of}")));
            match self.full(cx, p, old, &step).await {
                Ok(d) => self.say_digested(cx, &d),
                Err(CommandError::Cancelled) => return Err(CommandError::Cancelled),
                Err(CommandError::Failed { kind, .. })
                    if kind == ep_knowledge_contract::errors::NO_TEXT => {}
                // Without a model, or with one that does not answer, there is no point in going on.
                Err(e) => {
                    wrong += 1;
                    tracing::warn!(paper = %p.title, %e, "digest not finished");
                    let hopeless = matches!(&e, CommandError::Failed { kind, .. } if ["no_model", "no_runtime", "start_failed"].contains(&kind.as_str()));
                    if hopeless || wrong >= 3 {
                        return Err(e);
                    }
                }
            }
            self.set(cx, |s| {
                s.done += u32::from(!was_asked);
                s.paper = None;
                s.step = None;
            });
        }
        Ok(())
    }

    async fn infos(&self, cx: &Cx, i: Papers) -> Result<Vec<DigestInfo>, CommandError> {
        let items = self.library(cx, i.papers.as_deref()).await?;
        let have = self.stored(cx, None).await?;
        Ok(items
            .into_iter()
            .map(|p| {
                let d = have.iter().find(|d| d.paper == p.id);
                DigestInfo {
                    level: d.map(|d| d.level),
                    complete: d.is_some_and(|d| d.complete),
                    summary: d.map(|d| d.summary.clone()).unwrap_or_default(),
                    topics: d.map(|d| d.topics.clone()).unwrap_or_default(),
                    kind: d.map(|d| d.kind.clone()).unwrap_or_default(),
                    sections: d.map_or(0, |d| u32::try_from(d.sections.len()).unwrap_or(0)),
                    model: d.and_then(|d| d.model.clone()),
                    made_ms: d.map_or(0, |d| d.made_ms),
                    paper: p.id,
                }
            })
            .collect())
    }

    /// Register the commands on `bus`.
    ///
    /// # Errors
    /// Duplicate names.
    pub fn register(self: &Arc<Self>, bus: &Arc<Bus>) -> Result<(), RegisterError> {
        *self.bus.write() = Arc::downgrade(bus);
        macro_rules! reg {
            ($name:expr, $summary:expr, $perm:expr, |$me:ident, $cx:ident, $i:ident: $t:ty| $body:expr) => {{
                let me = self.clone();
                bus.register(
                    Spec {
                        name: $name,
                        summary: $summary,
                        stability: Stability::Experimental,
                        permission: $perm,
                    },
                    move |$cx, $i: $t| {
                        let $me = me.clone();
                        async move { $body }
                    },
                )?;
            }};
        }
        reg!(
            cmd::DIGEST_PAPER,
            "Make the digest of a paper: summaries, tags and terms of every section",
            Permission::create(),
            |me, cx, i: DigestRequest| me.digest_paper(&cx, i).await
        );
        reg!(
            cmd::DIGESTS,
            "What digests there are",
            Permission::read(),
            |me, cx, i: Papers| me.infos(&cx, i).await
        );
        reg!(
            cmd::GET_DIGEST,
            "The digest of a paper",
            Permission::read(),
            |me, cx, i: DigestRequest| {
                Ok(me.stored(&cx, Some(&i.paper.to_string())).await?.pop())
            }
        );
        reg!(
            cmd::DIGEST_LIBRARY,
            "Make digests of the library in the background",
            Permission::create(),
            |me, cx, i: DigestLibrary| Ok(me.start(&cx, &i))
        );
        reg!(
            cmd::DIGEST_STOP,
            "Stop making digests",
            Permission::read(),
            |me, _cx, _i: ()| {
                if let Some(t) = me.stop.lock().as_ref() {
                    t.cancel();
                }
                Ok(me.status.lock().clone())
            }
        );
        reg!(
            cmd::DIGEST_STATUS,
            "State of making digests",
            Permission::read(),
            |me, _cx, _i: ()| Ok(me.status.lock().clone())
        );
        reg!(
            cmd::FIND,
            "Ask the library: papers it can be in, and the nearest places",
            Permission::read(),
            |me, cx, i: FindRequest| me.find(&cx, i).await
        );
        Ok(())
    }
}
