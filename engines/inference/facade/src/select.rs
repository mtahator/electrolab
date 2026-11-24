//! Which model answers (contract: *One model, kept ready*): the user's order, the memory that is free,
//! and what could not be used before. Pure: it decides, the facade starts.

use std::collections::HashSet;

use ep_inference_contract::{Backend, ModelKind};

use crate::catalog::{self, Found};

/// Most models in the automatic order.
const AUTOMATIC: usize = 3;
/// Shorter context tried when the usual one does not fit.
pub const SHORT_CONTEXT: u32 = 6144;
/// Memory a runtime needs besides the weights and the context.
const WORKING: u64 = 280 << 20;

/// What to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub model: Found,
    pub context: u32,
    /// The whole model goes on the GPU; otherwise llama.cpp decides how much of it.
    pub whole: bool,
}

/// Memory a model needs on the GPU with this context (8-bit cache).
pub fn need(m: &Found, context: u32) -> u64 {
    let cache = match m.cache_per_token {
        Some(per_token) => per_token * u64::from(context),
        // Not said in the file: as a share of the weights.
        None => m.size / 4 * u64::from(context) / 8192,
    };
    m.size + m.size / 50 + cache + WORKING
}

/// The models in the order they are tried: the user's, else the suggested one and smaller ones.
pub fn order(chosen: &[String], models: &[Found], backend: Backend) -> Vec<Found> {
    let chat = || models.iter().filter(|m| m.kind == ModelKind::Chat);
    let own: Vec<Found> = chosen
        .iter()
        .filter_map(|id| chat().find(|m| m.id == *id).cloned())
        .collect();
    if !own.is_empty() {
        return own;
    }
    let Some(first) = catalog::suggested(models, backend) else {
        return vec![];
    };
    let mut smaller: Vec<&Found> = chat().filter(|m| m.size < first.size).collect();
    smaller.sort_by_key(|m| (!catalog::is_known(&m.id), std::cmp::Reverse(m.size)));
    std::iter::once(first)
        .chain(smaller)
        .take(AUTOMATIC)
        .cloned()
        .collect()
}

/// The first model that can be used now. `free` is the GPU's free memory, when known.
pub fn pick(order: &[Found], free: Option<u64>, skip: &HashSet<String>) -> Option<Pick> {
    let left = || order.iter().filter(|m| !skip.contains(&m.id));
    let Some(free) = free else {
        return left().next().map(|m| Pick {
            whole: m.size <= catalog::FITS_GPU,
            context: m.context,
            model: m.clone(),
        });
    };
    for m in left() {
        for context in [m.context, SHORT_CONTEXT.min(m.context)] {
            if need(m, context) <= free {
                return Some(Pick {
                    model: m.clone(),
                    context,
                    whole: true,
                });
            }
        }
    }
    // Nothing fits whole: the smallest, shared with the processor.
    left().min_by_key(|m| m.size).map(|m| Pick {
        model: m.clone(),
        context: m.context,
        whole: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn model(id: &str, mb: u64) -> Found {
        Found {
            id: id.into(),
            name: id.into(),
            path: PathBuf::from(id),
            size: mb << 20,
            licence: String::new(),
            kind: ModelKind::Chat,
            architecture: "llama".into(),
            quantisation: None,
            context: 8192,
            cache_per_token: Some(75_000),
        }
    }

    #[test]
    fn the_users_order_is_followed_and_gone_models_are_left_out() {
        let all = [model("a", 2400), model("b", 1200), model("c", 600)];
        let ids = |v: &[Found]| v.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        let own = ["c".to_string(), "gone".into(), "a".into()];
        assert_eq!(ids(&order(&own, &all, Backend::Vulkan)), ["c", "a"]);
        // Without one: the suggested model, then smaller ones.
        assert_eq!(ids(&order(&[], &all, Backend::Vulkan)), ["a", "b", "c"]);
        assert_eq!(
            ids(&order(&["gone".into()], &all, Backend::Vulkan)),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn the_first_that_fits_is_used_and_a_full_memory_means_the_next() {
        let list = [model("a", 2400), model("b", 1200)];
        let none = HashSet::new();
        let at = |mb: u64, skip: &HashSet<String>| {
            let p = pick(&list, Some(mb << 20), skip).unwrap();
            (p.model.id, p.context, p.whole)
        };
        assert_eq!(at(3700, &none), ("a".into(), 8192, true));
        // A little less: the same model with a shorter context.
        assert_eq!(at(3200, &none), ("a".into(), SHORT_CONTEXT, true));
        assert_eq!(at(2400, &none), ("b".into(), 8192, true), "memory is full");
        // One that could not be started is not tried again.
        assert_eq!(
            at(3700, &HashSet::from(["a".to_string()])),
            ("b".into(), 8192, true)
        );
        assert_eq!(
            at(300, &none),
            ("b".into(), 8192, false),
            "nothing fits whole"
        );
        assert!(pick(&list, None, &HashSet::from(["a".into(), "b".into()])).is_none());
        // Unknown memory: the first is tried.
        assert_eq!(pick(&list, None, &none).unwrap().model.id, "a");
    }
}
