//! **Deferred material realization** — a `WowModelMaterial` becomes an *asset* the first frame
//! something drawable is bound to it, not the moment a spawner builds it.
//!
//! Why: every material asset costs the GPU lane real per-frame CPU whether or not it is ever
//! drawn. Bevy's non-bindless `AsBindGroup` gives each material its own bind group and one
//! buffer per `#[uniform]` binding (two for this material), and wgpu's pass encoder allocates
//! and clears usage trackers sized to the device's *live* buffer and texture counts for every
//! render pass it encodes — a per-pass term that scales with residency, not with what is drawn.
//! Measured on the crowd rig (1929's follow-on, the `wgpu_bufs=` census): 9.9k materials,
//! 20.7k buffers and 10.2k bind groups alive at the Stormwind auction house with 36 entity
//! batches drawn, because the batch builder ([`super::batch::M2BatchMaterials`]) hands every
//! spawner its whole variant set — steady, the two interior lanes, their blend twins, the
//! depth-prime twin — and the variants a batch never switches to sat in the store forever.
//!
//! The mechanism: the builder reserves a handle (`Assets::reserve_handle`) and parks the built
//! value here, keyed by the reserved id. [`realize_bound`], in `Last`, walks the bound handles
//! and inserts the asset for every one whose entity is view-visible this frame — before
//! extraction, so the first drawn frame is the frame it was bound. A handle every holder has
//! dropped arrives as the store's own `AssetEvent::Unused` (it fires for a reserved id too) and
//! the parked value goes with it. Nothing about the handles changes: a part still stores and
//! swaps `Handle<WowModelMaterial>`s, and every reader that clones a material *before* binding
//! it — the portrait booth's relight twins, a spell kit's per-instance copy, the pipeline
//! warmer — calls [`realize`] first, which is a no-op for an asset that already exists.
//!
//! **The table is a process global, deliberately.** It is a side of `Assets<WowModelMaterial>`
//! — "reserved, value parked" — that the asset store has no slot for, and the builder that
//! needs it ([`super::model_material`]) is a free function with a two-dozen-argument signature
//! reached from seven lanes through three caches (the engine's, the streamer's, the warmer's
//! `Local`). Threading one more resource through all of them for what is a property of the
//! store, not of any cache, is the wrong shape; one mutex, locked once per system run, is the
//! right one. Main-world only, like the store it shadows.

use std::collections::HashMap;
use std::sync::Mutex;

use bevy::asset::{Asset, AssetId};
use bevy::camera::visibility::ViewVisibility;
use bevy::prelude::*;

use benilla_assets::materials::WowModelMaterial;

/// The parked values of one asset type: built, handle reserved, not yet in the store. Generic
/// so the table's law is testable on a stub asset (a real `WowModelMaterial` carries a GPU
/// buffer and cannot exist without a device).
pub struct Parked<A: Asset>(HashMap<AssetId<A>, A>);

impl<A: Asset> Default for Parked<A> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<A: Asset> Parked<A> {
    /// Reserve a handle for `asset` and park the value.
    pub fn defer(&mut self, store: &Assets<A>, asset: A) -> Handle<A> {
        let handle = store.reserve_handle();
        self.0.insert(handle.id(), asset);
        handle
    }

    /// Make the asset behind `id` exist now, if its value is parked. Returns whether the store
    /// holds it afterwards — `false` only for an id this table never saw and the store does not
    /// have (a foreign handle, or one already dropped everywhere).
    pub fn realize(&mut self, store: &mut Assets<A>, id: AssetId<A>) -> bool {
        if store.contains(id) {
            return true;
        }
        let Some(asset) = self.0.remove(&id) else {
            return false;
        };
        // `Err` = the reserved index was dropped and re-minted with a new generation since this
        // value was parked; the `Unused` purge simply has not run yet. Nothing to insert.
        store.insert(id, asset).is_ok()
    }

    /// Insert every parked value.
    pub fn realize_all(&mut self, store: &mut Assets<A>) {
        for (id, asset) in self.0.drain() {
            let _ = store.insert(id, asset);
        }
    }

    /// The store reported `id` unused: whoever held the handle is gone, and so is the value.
    pub fn purge(&mut self, id: AssetId<A>) {
        self.0.remove(&id);
    }

    /// Realize every parked value some *visible* binding names. `bound` yields each binding
    /// with its view-visibility (`None` = not on the visibility lane at all — a booth part
    /// before its camera, a test world — which counts as visible: bound is enough).
    pub fn realize_visible(
        &mut self,
        store: &mut Assets<A>,
        bound: impl IntoIterator<Item = (AssetId<A>, Option<bool>)>,
    ) {
        if self.0.is_empty() {
            return;
        }
        for (id, visible) in bound {
            if visible == Some(false) || store.contains(id) {
                continue;
            }
            if let Some(asset) = self.0.remove(&id) {
                let _ = store.insert(id, asset);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

static PENDING: Mutex<Option<Parked<WowModelMaterial>>> = Mutex::new(None);

fn with_pending<R>(f: impl FnOnce(&mut Parked<WowModelMaterial>) -> R) -> R {
    let mut guard = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Parked::default))
}

/// Reserve a handle for `material` and park the value until something bound to the handle is
/// drawn. The builder's replacement for `Assets::add`.
pub fn defer(
    materials: &Assets<WowModelMaterial>,
    material: WowModelMaterial,
) -> Handle<WowModelMaterial> {
    with_pending(|p| p.defer(materials, material))
}

/// Make the asset behind `id` exist now — see [`Parked::realize`].
pub fn realize(materials: &mut Assets<WowModelMaterial>, id: AssetId<WowModelMaterial>) -> bool {
    with_pending(|p| p.realize(materials, id))
}

/// Insert every parked value. For a lane that reads materials back right after building them
/// (the pipeline warmer, which inspects each variant it built to derive its far-side twins).
pub fn realize_all(materials: &mut Assets<WowModelMaterial>) {
    with_pending(|p| p.realize_all(materials));
}

/// How many values are parked — the census figure beside `mats=`.
pub fn pending_len() -> usize {
    with_pending(|p| p.len())
}

/// `Last`: realize the material of every bound, view-visible entity, and drop the parked value
/// of every handle the store reports unused. After `PostUpdate` so a twin spawned there (the
/// depth-prime lane) is bound and visible in the same walk; before extraction by construction.
pub(super) fn realize_bound(
    bound: Query<(&MeshMaterial3d<WowModelMaterial>, Option<&ViewVisibility>)>,
    mut materials: ResMut<Assets<WowModelMaterial>>,
    mut unused: MessageReader<AssetEvent<WowModelMaterial>>,
) {
    with_pending(|p| {
        for event in unused.read() {
            if let AssetEvent::Unused { id } = event {
                p.purge(*id);
            }
        }
        p.realize_visible(
            &mut materials,
            bound.iter().map(|(m, v)| (m.0.id(), v.map(|v| v.get()))),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Asset, TypePath)]
    struct Stub(u8);

    #[test]
    fn deferred_asset_is_absent_until_realized() {
        let mut store = Assets::<Stub>::default();
        let mut parked = Parked::default();
        let handle = parked.defer(&store, Stub(1));
        assert!(!store.contains(handle.id()));
        assert_eq!(parked.len(), 1);
        assert!(parked.realize(&mut store, handle.id()));
        assert_eq!(store.get(&handle).map(|s| s.0), Some(1));
        assert!(parked.is_empty());
        // Realizing again is a no-op that still reports the asset present.
        assert!(parked.realize(&mut store, handle.id()));
        // An id nobody parked and the store lacks: false, nothing inserted.
        let stray = store.reserve_handle();
        assert!(!parked.realize(&mut store, stray.id()));
    }

    #[test]
    fn bound_and_visible_realizes_in_the_walk_hidden_does_not() {
        let mut store = Assets::<Stub>::default();
        let mut parked = Parked::default();
        let shown = parked.defer(&store, Stub(1));
        let hidden = parked.defer(&store, Stub(2));
        let unlaned = parked.defer(&store, Stub(3));
        parked.realize_visible(
            &mut store,
            [
                (shown.id(), Some(true)),
                (hidden.id(), Some(false)),
                (unlaned.id(), None),
            ],
        );
        assert!(store.contains(shown.id()), "visible ⇒ realized");
        assert!(!store.contains(hidden.id()), "hidden ⇒ still parked");
        assert!(
            store.contains(unlaned.id()),
            "no visibility lane ⇒ bound is enough"
        );
        assert_eq!(parked.len(), 1);
        parked.purge(hidden.id());
        assert!(
            parked.is_empty(),
            "the store's Unused drops the parked value"
        );
    }
}
