//! Knowledge engine facade: digests of papers, made in the background and kept with them, and asking
//! the library (`engines/knowledge/CONTRACT.md`). It calls Document, Inference and Workspace over the
//! bus only.

mod digest;
mod find;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use ep_bus::{Bus, CommandError, Cx, Envelope, Permission, RegisterError, Spec, Stability};
pub use ep_knowledge_contract as contract;
use ep_knowledge_contract::{
    DIGEST_VERSION, Digest, DigestInfo, DigestLibrary, DigestRequest, DigestStatus, FindRequest,
    Level, PaperId, Papers, Waiting, commands as cmd, events,
};
use ep_workspace_contract::{Paper, PaperDigest};
use parking_lot::{Mutex, RwLock};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use digest::{GAVE_WAY, Item};

/// How often the battery is looked at again.
const BATTERY_LOOK: std::time::Duration = std::time::Duration::from_secs(5);

/// The engine.
#[derive(Default)]
pub struct Knowledge {
    bus: RwLock<Weak<Bus>>,
    status: Mutex<DigestStatus>,
    stop: Mutex<Option<CancellationToken>>,
    /// Papers whose digest is made anew when their turn comes.
    anew: Mutex<HashSet<String>>,
    /// Papers a person asked for (the others were put in the row by themselves).
    asked: Mutex<HashSet<String>>,
    /// How often papers were named; a paper at work gives way to one named after it was begun.
    named: AtomicU64,
    /// Papers that could not be understood since the app started, and why.
    problems: Mutex<HashMap<String, String>>,
    machine: RwLock<Option<Arc<dyn ep_os::Machine>>>,
}

/// Is a digest finished, and of the file as it is now?
fn is_ready(d: &Digest, item: &Item) -> bool {
    d.level == Level::Full
        && d.complete
        && d.file
            .as_ref()
            .is_none_or(|f| *f == item.file.hash.to_string())
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

    /// Ask `machine` whether it is low on power.
    pub fn use_machine(&self, machine: Arc<dyn ep_os::Machine>) {
        *self.machine.write() = Some(machine);
    }

    fn low_on_power(&self) -> bool {
        let m = self.machine.read().clone();
        m.and_then(|m| m.low_on_power()).unwrap_or(false)
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
            && is_ready(d, &item)
        {
            return Ok(d.clone());
        }
        let d = self.full(cx, &item, old, &|_, _| true).await?;
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
        {
            let mut stop = self.stop.lock();
            let mut status = self.status.lock();
            if i.auto && status.stopped {
                return status.clone();
            }
            if !named.is_empty() {
                self.named.fetch_add(1, Ordering::SeqCst);
            }
            for (n, id) in named.iter().enumerate() {
                if i.force {
                    self.anew.lock().insert(id.clone());
                }
                self.problems.lock().remove(id);
                let at_work = status.paper.as_ref() == Some(id) && !i.force;
                if i.auto {
                    // The paper opened last goes first.
                    if !at_work {
                        status.next.retain(|x| x != id);
                        let n = n.min(status.next.len());
                        status.next.insert(n, id.clone());
                    }
                } else {
                    self.asked.lock().insert(id.clone());
                    if !status.next.contains(id) && !at_work {
                        status.next.push(id.clone());
                    }
                }
            }
            if status.running {
                status.full |= i.full;
                return status.clone();
            }
            *stop = Some(CancellationToken::new());
            *status = DigestStatus {
                running: true,
                full: i.full,
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
            me.asked.lock().clear();
            me.set(&own, |s| {
                s.running = false;
                s.full = false;
                s.paper = None;
                s.next.clear();
                s.current = None;
                s.step = None;
                s.waiting = None;
                s.error = match r {
                    Ok(()) | Err(CommandError::Cancelled) => None,
                    Err(CommandError::Failed { kind, .. })
                        if kind == "no_model" || kind == "no_runtime" =>
                    {
                        Some(kind)
                    }
                    Err(e) => Some(e.to_string()),
                };
            });
        });
        self.status.lock().clone()
    }

    /// Wait while the computer is low on power and nobody asked for this paper.
    async fn wait_for_power(&self, cx: &Cx, id: &str) -> Result<(), CommandError> {
        while self.low_on_power() && !self.asked.lock().contains(id) {
            if self.status.lock().waiting.is_none() {
                self.set(cx, |s| s.waiting = Some(Waiting::Battery));
            }
            tokio::select! {
                () = tokio::time::sleep(BATTERY_LOOK) => {}
                () = cx.cancel.cancelled() => return Err(CommandError::Cancelled),
            }
            // Something a person asked for goes first.
            let asked = self.asked.lock().clone();
            if self.status.lock().next.iter().any(|n| asked.contains(n)) {
                return Err(failed(GAVE_WAY, "another paper goes first"));
            }
        }
        if self.status.lock().waiting.is_some() {
            self.set(cx, |s| s.waiting = None);
        }
        Ok(())
    }

    /// The quick digests, of every paper that has none (or a changed file).
    async fn quick_ones(&self, cx: &Cx, items: &[Item]) -> Result<(), CommandError> {
        let have: HashMap<String, Digest> = self
            .stored(cx, None)
            .await?
            .into_iter()
            .map(|d| (d.paper.clone(), d))
            .collect();
        for p in items.iter().filter(|p| !have.contains_key(&p.id)) {
            if cx.is_cancelled() {
                return Err(CommandError::Cancelled);
            }
            if self.problems.lock().contains_key(&p.id) {
                continue;
            }
            self.set(cx, |s| s.current = Some(p.title.clone()));
            match self.basic(cx, p).await {
                Ok((d, _)) => {
                    self.keep(cx, &d).await?;
                    self.say_digested(cx, &d);
                }
                Err(CommandError::Cancelled) => return Err(CommandError::Cancelled),
                // A scanned or damaged paper does not hold up the others.
                Err(e) => self.problem(p, &e),
            }
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    fn problem(&self, p: &Item, e: &CommandError) {
        tracing::info!(paper = %p.title, %e, "not understood");
        let kind = match e {
            CommandError::Failed { kind, .. } => kind.clone(),
            e => e.to_string(),
        };
        self.problems.lock().insert(p.id.clone(), kind);
    }

    async fn make_all(&self, cx: &Cx) -> Result<(), CommandError> {
        let mut wrong = 0;
        loop {
            if cx.is_cancelled() {
                return Err(CommandError::Cancelled);
            }
            // Looked at anew every time: papers are added, opened and removed meanwhile.
            let items = self.library(cx, None).await?;
            self.quick_ones(cx, &items).await?;
            let stored = self.stored(cx, None).await?;
            let ready = |p: &Item| stored.iter().any(|d| d.paper == p.id && is_ready(d, p));
            let possible = |p: &Item| !self.problems.lock().contains_key(&p.id);
            let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
            self.set(cx, |s| {
                s.total = count(items.iter().filter(|p| possible(p)).count());
                s.done = count(items.iter().filter(|p| ready(p)).count());
                s.current = None;
                s.paper = None;
                s.step = None;
            });
            // Those named first, then (when all are wanted) the one opened last.
            let (named, all) = {
                let mut s = self.status.lock();
                let named = (!s.next.is_empty()).then(|| s.next.remove(0));
                (named, s.full)
            };
            let was_named = named.is_some();
            let p = match named {
                Some(id) => items.iter().find(|p| p.id == id),
                None if all => items.iter().find(|p| !ready(p) && possible(p)),
                None => None,
            };
            let Some(p) = p else {
                if was_named {
                    continue;
                }
                return Ok(());
            };
            let anew = self.anew.lock().remove(&p.id);
            if ready(p) && !anew {
                continue;
            }
            let old = stored
                .iter()
                .find(|d| d.paper == p.id)
                .filter(|_| !anew)
                .cloned();
            self.set(cx, |s| {
                s.current = Some(p.title.clone());
                s.paper = Some(p.id.clone());
            });
            let mine = self.asked.lock().contains(&p.id);
            let begun = self.named.load(Ordering::SeqCst);
            let step = |n: usize, of: usize| {
                self.set(cx, |s| s.step = Some(format!("{n} / {of}")));
                let s = self.status.lock();
                // A paper nobody asked for gives way to one that was opened or asked for.
                let later = self.named.load(Ordering::SeqCst) != begun && !s.next.is_empty();
                mine || (!later && (was_named || s.full))
            };
            let made = match self.wait_for_power(cx, &p.id).await {
                Ok(()) => self.full(cx, p, old, &step).await,
                Err(e) => Err(e),
            };
            match made {
                Ok(d) => {
                    wrong = 0;
                    self.asked.lock().remove(&p.id);
                    self.say_digested(cx, &d);
                }
                Err(CommandError::Cancelled) => return Err(CommandError::Cancelled),
                Err(CommandError::Failed { kind, .. }) if kind == GAVE_WAY => {
                    // A paper that was opened is gone on with afterwards.
                    if was_named {
                        let mut s = self.status.lock();
                        if !s.next.contains(&p.id) {
                            s.next.push(p.id.clone());
                        }
                    }
                }
                // Without a model there is no point in going on.
                Err(e) => {
                    let hopeless = matches!(&e, CommandError::Failed { kind, .. } if ["no_model", "no_runtime", "start_failed"].contains(&kind.as_str()));
                    self.problem(p, &e);
                    let no_text = matches!(&e, CommandError::Failed { kind, .. } if kind == ep_knowledge_contract::errors::NO_TEXT);
                    wrong += u32::from(!no_text);
                    if hopeless {
                        self.problems.lock().remove(&p.id);
                    }
                    if hopeless || wrong >= 3 {
                        return Err(e);
                    }
                }
            }
        }
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
                    ready: d.is_some_and(|d| is_ready(d, &p)),
                    problem: self.problems.lock().get(&p.id).cloned(),
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
                let mut s = me.status.lock();
                s.stopped = true;
                Ok(s.clone())
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
