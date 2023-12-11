use crate::dep_graph;
use crate::dep_graph::DepKind;
use crate::query::on_disk_cache::CacheEncoder;
use crate::query::on_disk_cache::EncodedDepNodeIndex;
use crate::query::on_disk_cache::OnDiskCache;
use crate::query::{
    DynamicQueries, ExternProviders, Providers, QueryArenas, QueryCaches, QueryEngine, QueryStates,
};
use crate::ty::TyCtxt;
use field_offset::FieldOffset;
use measureme::StringId;
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::sync::AtomicU64;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_hir::hir_id::OwnerId;
use rustc_query_system::dep_graph::DepNodeIndex;
use rustc_query_system::dep_graph::SerializedDepNodeIndex;
pub(crate) use rustc_query_system::query::QueryJobId;
use rustc_query_system::query::*;
use rustc_query_system::HandleCycleError;
use rustc_span::{Span, DUMMY_SP};
use std::ops::Deref;

pub struct QueryKeyStringCache {
    pub def_id_cache: FxHashMap<DefId, StringId>,
}

impl QueryKeyStringCache {
    pub fn new() -> QueryKeyStringCache {
        QueryKeyStringCache { def_id_cache: Default::default() }
    }
}

pub struct DynamicQuery<'tcx, C: QueryCache> {
    pub name: &'static str,
    pub eval_always: bool,
    pub dep_kind: DepKind,
    pub handle_cycle_error: HandleCycleError,
    pub query_state: FieldOffset<QueryStates<'tcx>, QueryState<C::Key, DepKind>>,
    pub query_cache: FieldOffset<QueryCaches<'tcx>, C>,
    pub cache_on_disk: fn(tcx: TyCtxt<'tcx>, key: &C::Key) -> bool,
    pub execute_query: fn(tcx: TyCtxt<'tcx>, k: C::Key) -> C::Value,
    pub compute: fn(tcx: TyCtxt<'tcx>, key: C::Key) -> C::Value,
    pub can_load_from_disk: bool,
    pub try_load_from_disk: fn(
        tcx: TyCtxt<'tcx>,
        key: &C::Key,
        prev_index: SerializedDepNodeIndex,
        index: DepNodeIndex,
    ) -> Option<C::Value>,
    pub loadable_from_disk:
        fn(tcx: TyCtxt<'tcx>, key: &C::Key, index: SerializedDepNodeIndex) -> bool,
    pub hash_result: HashResult<C::Value>,
    pub value_from_cycle_error: fn(tcx: TyCtxt<'tcx>, cycle: &[QueryInfo<DepKind>]) -> C::Value,
    pub format_value: fn(&C::Value) -> String,
}

pub struct QuerySystemFns<'tcx> {
    pub engine: QueryEngine,
    pub local_providers: Providers,
    pub extern_providers: ExternProviders,
    pub encode_query_results: fn(
        tcx: TyCtxt<'tcx>,
        encoder: &mut CacheEncoder<'_, 'tcx>,
        query_result_index: &mut EncodedDepNodeIndex,
    ),
    pub try_mark_green: fn(tcx: TyCtxt<'tcx>, dep_node: &dep_graph::DepNode) -> bool,
}

pub struct QuerySystem<'tcx> {
    pub states: QueryStates<'tcx>,
    pub arenas: QueryArenas<'tcx>,
    pub caches: QueryCaches<'tcx>,
    pub dynamic_queries: DynamicQueries<'tcx>,

    /// This provides access to the incremental compilation on-disk cache for query results.
    /// Do not access this directly. It is only meant to be used by
    /// `DepGraph::try_mark_green()` and the query infrastructure.
    /// This is `None` if we are not incremental compilation mode
    pub on_disk_cache: Option<OnDiskCache<'tcx>>,

    pub fns: QuerySystemFns<'tcx>,

    pub jobs: AtomicU64,
}

#[derive(Copy, Clone)]
pub struct TyCtxtAt<'tcx> {
    pub tcx: TyCtxt<'tcx>,
    pub span: Span,
}

impl<'tcx> Deref for TyCtxtAt<'tcx> {
    type Target = TyCtxt<'tcx>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.tcx
    }
}

#[derive(Copy, Clone)]
pub struct TyCtxtEnsure<'tcx> {
    pub tcx: TyCtxt<'tcx>,
}

#[derive(Copy, Clone)]
pub struct TyCtxtEnsureWithValue<'tcx> {
    pub tcx: TyCtxt<'tcx>,
}

impl<'tcx> TyCtxt<'tcx> {
    /// Returns a transparent wrapper for `TyCtxt`, which ensures queries
    /// are executed instead of just returning their results.
    #[inline(always)]
    pub fn ensure(self) -> TyCtxtEnsure<'tcx> {
        TyCtxtEnsure { tcx: self }
    }

    /// Returns a transparent wrapper for `TyCtxt`, which ensures queries
    /// are executed instead of just returning their results.
    ///
    /// This version verifies that the computed result exists in the cache before returning.
    #[inline(always)]
    pub fn ensure_with_value(self) -> TyCtxtEnsureWithValue<'tcx> {
        TyCtxtEnsureWithValue { tcx: self }
    }

    /// Returns a transparent wrapper for `TyCtxt` which uses
    /// `span` as the location of queries performed through it.
    #[inline(always)]
    pub fn at(self, span: Span) -> TyCtxtAt<'tcx> {
        TyCtxtAt { tcx: self, span }
    }

    pub fn try_mark_green(self, dep_node: &dep_graph::DepNode) -> bool {
        (self.query_system.fns.try_mark_green)(self, dep_node)
    }
}

#[inline]
pub fn query_get_at<'tcx, Cache>(
    tcx: TyCtxt<'tcx>,
    execute_query: fn(TyCtxt<'tcx>, Span, Cache::Key, QueryMode) -> Option<Cache::Value>,
    query_cache: &Cache,
    span: Span,
    key: Cache::Key,
) -> Cache::Value
where
    Cache: QueryCache,
{
    let key = key.into_query_param();
    match try_get_cached(tcx, query_cache, &key) {
        Some(value) => value,
        None => execute_query(tcx, span, key, QueryMode::Get).unwrap(),
    }
}

#[inline]
pub fn query_ensure<'tcx, Cache>(
    tcx: TyCtxt<'tcx>,
    execute_query: fn(TyCtxt<'tcx>, Span, Cache::Key, QueryMode) -> Option<Cache::Value>,
    query_cache: &Cache,
    key: Cache::Key,
    check_cache: bool,
) where
    Cache: QueryCache,
{
    let key = key.into_query_param();
    if try_get_cached(tcx, query_cache, &key).is_none() {
        execute_query(tcx, DUMMY_SP, key, QueryMode::Ensure { check_cache });
    }
}

macro_rules! query_helper_param_ty {
    (DefId) => { impl IntoQueryParam<DefId> };
    (LocalDefId) => { impl IntoQueryParam<LocalDefId> };
    ($K:ty) => { $K };
}

macro_rules! query_if_arena {
    ([] $arena:tt $no_arena:tt) => {
        $no_arena
    };
    ([(arena_cache) $($rest:tt)*] $arena:tt $no_arena:tt) => {
        $arena
    };
    ([$other:tt $($modifiers:tt)*]$($args:tt)*) => {
        query_if_arena!([$($modifiers)*]$($args)*)
    };
}

/// If `separate_provide_if_extern`, then the key can be projected to its
/// local key via `<$K as AsLocalKey>::LocalKey`.
macro_rules! local_key_if_separate_extern {
    ([] $($K:tt)*) => {
        $($K)*
    };
    ([(separate_provide_extern) $($rest:tt)*] $($K:tt)*) => {
        <$($K)* as AsLocalKey>::LocalKey
    };
    ([$other:tt $($modifiers:tt)*] $($K:tt)*) => {
        local_key_if_separate_extern!([$($modifiers)*] $($K)*)
    };
}

macro_rules! separate_provide_extern_decl {
    ([][$name:ident]) => {
        ()
    };
    ([(separate_provide_extern) $($rest:tt)*][$name:ident]) => {
        for<'tcx> fn(
            TyCtxt<'tcx>,
            queries::$name::Key<'tcx>,
        ) -> queries::$name::ProvidedValue<'tcx>
    };
    ([$other:tt $($modifiers:tt)*][$($args:tt)*]) => {
        separate_provide_extern_decl!([$($modifiers)*][$($args)*])
    };
}

macro_rules! separate_provide_extern_default {
    ([][$name:ident]) => {
        ()
    };
    ([(separate_provide_extern) $($rest:tt)*][$name:ident]) => {
        |_, key| bug!(
            "`tcx.{}({:?})` unsupported by its crate; \
             perhaps the `{}` query was never assigned a provider function",
            stringify!($name),
            key,
            stringify!($name),
        )
    };
    ([$other:tt $($modifiers:tt)*][$($args:tt)*]) => {
        separate_provide_extern_default!([$($modifiers)*][$($args)*])
    };
}

macro_rules! define_callbacks {
    (
     $($(#[$attr:meta])*
        [$($modifiers:tt)*] fn $name:ident($($K:tt)*) -> $V:ty,)*) => {

        #[allow(unused_lifetimes)]
        pub mod queries {
            $(pub mod $name {
                use super::super::*;

                pub type Key<'tcx> = $($K)*;
                pub type Value<'tcx> = $V;

                pub type LocalKey<'tcx> = local_key_if_separate_extern!([$($modifiers)*] $($K)*);

                /// This type alias specifies the type returned from query providers and the type
                /// used for decoding. For regular queries this is the declared returned type `V`,
                /// but `arena_cache` will use `<V as Deref>::Target` instead.
                pub type ProvidedValue<'tcx> = query_if_arena!(
                    [$($modifiers)*]
                    (<$V as Deref>::Target)
                    ($V)
                );

                /// This function takes `ProvidedValue` and coverts it to an erased `Value` by
                /// allocating it on an arena if the query has the `arena_cache` modifier. The
                /// value is then erased and returned. This will happen when computing the query
                /// using a provider or decoding a stored result.
                #[inline(always)]
                pub fn provided_to_erased<'tcx>(
                    _tcx: TyCtxt<'tcx>,
                    value: ProvidedValue<'tcx>,
                ) -> Erase<Value<'tcx>> {
                    erase(query_if_arena!([$($modifiers)*]
                        {
                            if mem::needs_drop::<ProvidedValue<'tcx>>() {
                                &*_tcx.query_system.arenas.$name.alloc(value)
                            } else {
                                &*_tcx.arena.dropless.alloc(value)
                            }
                        }
                        (value)
                    ))
                }

                pub type Storage<'tcx> = <
                    <$($K)* as keys::Key>::CacheSelector as CacheSelector<'tcx, Erase<$V>>
                >::Cache;

                // Ensure that keys grow no larger than 72 bytes
                #[cfg(all(target_arch = "x86_64", target_pointer_width = "64"))]
                const _: () = {
                    if mem::size_of::<Key<'static>>() > 72 {
                        panic!("{}", concat!(
                            "the query `",
                            stringify!($name),
                            "` has a key type `",
                            stringify!($($K)*),
                            "` that is too large"
                        ));
                    }
                };

                // Ensure that values grow no larger than 64 bytes
                #[cfg(all(target_arch = "x86_64", target_pointer_width = "64"))]
                const _: () = {
                    if mem::size_of::<Value<'static>>() > 64 {
                        panic!("{}", concat!(
                            "the query `",
                            stringify!($name),
                            "` has a value type `",
                            stringify!($V),
                            "` that is too large"
                        ));
                    }
                };
            })*
        }

        pub struct QueryArenas<'tcx> {
            $($(#[$attr])* pub $name: query_if_arena!([$($modifiers)*]
                (WorkerLocal<TypedArena<<$V as Deref>::Target>>)
                ()
            ),)*
        }

        impl Default for QueryArenas<'_> {
            fn default() -> Self {
                Self {
                    $($name: query_if_arena!([$($modifiers)*]
                        (WorkerLocal::new(|_| Default::default()))
                        ()
                    ),)*
                }
            }
        }

        #[derive(Default)]
        pub struct QueryCaches<'tcx> {
            $($(#[$attr])* pub $name: queries::$name::Storage<'tcx>,)*
        }

        impl<'tcx> TyCtxtEnsure<'tcx> {
            $($(#[$attr])*
            #[inline(always)]
            pub fn $name(self, key: query_helper_param_ty!($($K)*)) {
                query_ensure(
                    self.tcx,
                    self.tcx.query_system.fns.engine.$name,
                    &self.tcx.query_system.caches.$name,
                    key.into_query_param(),
                    false,
                );
            })*
        }

        impl<'tcx> TyCtxtEnsureWithValue<'tcx> {
            $($(#[$attr])*
            #[inline(always)]
            pub fn $name(self, key: query_helper_param_ty!($($K)*)) {
                query_ensure(
                    self.tcx,
                    self.tcx.query_system.fns.engine.$name,
                    &self.tcx.query_system.caches.$name,
                    key.into_query_param(),
                    true,
                );
            })*
        }

        impl<'tcx> TyCtxt<'tcx> {
            $($(#[$attr])*
            #[inline(always)]
            #[must_use]
            pub fn $name(self, key: query_helper_param_ty!($($K)*)) -> $V
            {
                self.at(DUMMY_SP).$name(key)
            })*
        }

        impl<'tcx> TyCtxtAt<'tcx> {
            $($(#[$attr])*
            #[inline(always)]
            pub fn $name(self, key: query_helper_param_ty!($($K)*)) -> $V
            {
                restore::<$V>(query_get_at(
                    self.tcx,
                    self.tcx.query_system.fns.engine.$name,
                    &self.tcx.query_system.caches.$name,
                    self.span,
                    key.into_query_param(),
                ))
            })*
        }

        pub struct DynamicQueries<'tcx> {
            $(
                pub $name: DynamicQuery<'tcx, queries::$name::Storage<'tcx>>,
            )*
        }

        #[derive(Default)]
        pub struct QueryStates<'tcx> {
            $(
                pub $name: QueryState<$($K)*, DepKind>,
            )*
        }

        pub struct Providers {
            $(pub $name: for<'tcx> fn(
                TyCtxt<'tcx>,
                queries::$name::LocalKey<'tcx>,
            ) -> queries::$name::ProvidedValue<'tcx>,)*
        }

        pub struct ExternProviders {
            $(pub $name: separate_provide_extern_decl!([$($modifiers)*][$name]),)*
        }

        impl Default for Providers {
            fn default() -> Self {
                Providers {
                    $($name: |_, key| bug!(
                        "`tcx.{}({:?})` is not supported for this key;\n\
                        hint: Queries can be either made to the local crate, or the external crate. \
                        This error means you tried to use it for one that's not supported.\n\
                        If that's not the case, {} was likely never assigned to a provider function.\n",
                        stringify!($name),
                        key,
                        stringify!($name),
                    ),)*
                }
            }
        }

        impl Default for ExternProviders {
            fn default() -> Self {
                ExternProviders {
                    $($name: separate_provide_extern_default!([$($modifiers)*][$name]),)*
                }
            }
        }

        impl Copy for Providers {}
        impl Clone for Providers {
            fn clone(&self) -> Self { *self }
        }

        impl Copy for ExternProviders {}
        impl Clone for ExternProviders {
            fn clone(&self) -> Self { *self }
        }

        pub struct QueryEngine {
            $(pub $name: for<'tcx> fn(
                TyCtxt<'tcx>,
                Span,
                queries::$name::Key<'tcx>,
                QueryMode,
            ) -> Option<Erase<$V>>,)*
        }
    };
}

macro_rules! hash_result {
    ([]) => {{
        Some(dep_graph::hash_result)
    }};
    ([(no_hash) $($rest:tt)*]) => {{
        None
    }};
    ([$other:tt $($modifiers:tt)*]) => {
        hash_result!([$($modifiers)*])
    };
}

macro_rules! define_feedable {
    ($($(#[$attr:meta])* [$($modifiers:tt)*] fn $name:ident($($K:tt)*) -> $V:ty,)*) => {
        $(impl<'tcx, K: IntoQueryParam<$($K)*> + Copy> TyCtxtFeed<'tcx, K> {
            $(#[$attr])*
            #[inline(always)]
            pub fn $name(self, value: queries::$name::ProvidedValue<'tcx>) {
                let key = self.key().into_query_param();

                let tcx = self.tcx;
                let erased = queries::$name::provided_to_erased(tcx, value);
                let value = restore::<$V>(erased);
                let cache = &tcx.query_system.caches.$name;

                let hasher: Option<fn(&mut StableHashingContext<'_>, &_) -> _> = hash_result!([$($modifiers)*]);
                match try_get_cached(tcx, cache, &key) {
                    Some(old) => {
                        let old = restore::<$V>(old);
                        if let Some(hasher) = hasher {
                            let (value_hash, old_hash): (Fingerprint, Fingerprint) = tcx.with_stable_hashing_context(|mut hcx|
                                (hasher(&mut hcx, &value), hasher(&mut hcx, &old))
                            );
                            if old_hash != value_hash {
                                // We have an inconsistency. This can happen if one of the two
                                // results is tainted by errors. In this case, delay a bug to
                                // ensure compilation is doomed, and keep the `old` value.
                                tcx.sess.delay_span_bug(DUMMY_SP, format!(
                                    "Trying to feed an already recorded value for query {} key={key:?}:\n\
                                    old value: {old:?}\nnew value: {value:?}",
                                    stringify!($name),
                                ));
                            }
                        } else {
                            // The query is `no_hash`, so we have no way to perform a sanity check.
                            // If feeding the same value multiple times needs to be supported,
                            // the query should not be marked `no_hash`.
                            bug!(
                                "Trying to feed an already recorded value for query {} key={key:?}:\nold value: {old:?}\nnew value: {value:?}",
                                stringify!($name),
                            )
                        }
                    }
                    None => {
                        let dep_node = dep_graph::DepNode::construct(tcx, dep_graph::DepKind::$name, &key);
                        let dep_node_index = tcx.dep_graph.with_feed_task(
                            dep_node,
                            tcx,
                            key,
                            &value,
                            hash_result!([$($modifiers)*]),
                        );
                        cache.complete(key, erased, dep_node_index);
                    }
                }
            }
        })*
    }
}

// Each of these queries corresponds to a function pointer field in the
// `Providers` struct for requesting a value of that type, and a method
// on `tcx: TyCtxt` (and `tcx.at(span)`) for doing that request in a way
// which memoizes and does dep-graph tracking, wrapping around the actual
// `Providers` that the driver creates (using several `rustc_*` crates).
//
// The result type of each query must implement `Clone`, and additionally
// `ty::query::values::Value`, which produces an appropriate placeholder
// (error) value if the query resulted in a query cycle.
// Queries marked with `fatal_cycle` do not need the latter implementation,
// as they will raise an fatal error on query cycles instead.

mod sealed {
    use super::{DefId, LocalDefId, OwnerId};
    use rustc_hir::def_id::{LocalModDefId, ModDefId};

    /// An analogue of the `Into` trait that's intended only for query parameters.
    ///
    /// This exists to allow queries to accept either `DefId` or `LocalDefId` while requiring that the
    /// user call `to_def_id` to convert between them everywhere else.
    pub trait IntoQueryParam<P> {
        fn into_query_param(self) -> P;
    }

    impl<P> IntoQueryParam<P> for P {
        #[inline(always)]
        fn into_query_param(self) -> P {
            self
        }
    }

    impl<'a, P: Copy> IntoQueryParam<P> for &'a P {
        #[inline(always)]
        fn into_query_param(self) -> P {
            *self
        }
    }

    impl IntoQueryParam<LocalDefId> for OwnerId {
        #[inline(always)]
        fn into_query_param(self) -> LocalDefId {
            self.def_id
        }
    }

    impl IntoQueryParam<DefId> for LocalDefId {
        #[inline(always)]
        fn into_query_param(self) -> DefId {
            self.to_def_id()
        }
    }

    impl IntoQueryParam<DefId> for OwnerId {
        #[inline(always)]
        fn into_query_param(self) -> DefId {
            self.to_def_id()
        }
    }

    impl IntoQueryParam<DefId> for ModDefId {
        #[inline(always)]
        fn into_query_param(self) -> DefId {
            self.to_def_id()
        }
    }

    impl IntoQueryParam<DefId> for LocalModDefId {
        #[inline(always)]
        fn into_query_param(self) -> DefId {
            self.to_def_id()
        }
    }

    impl IntoQueryParam<LocalDefId> for LocalModDefId {
        #[inline(always)]
        fn into_query_param(self) -> LocalDefId {
            self.into()
        }
    }
}

pub use sealed::IntoQueryParam;

impl<'tcx> TyCtxt<'tcx> {
    pub fn def_kind(self, def_id: impl IntoQueryParam<DefId>) -> DefKind {
        let def_id = def_id.into_query_param();
        self.opt_def_kind(def_id)
            .unwrap_or_else(|| bug!("def_kind: unsupported node: {:?}", def_id))
    }
}

impl<'tcx> TyCtxtAt<'tcx> {
    pub fn def_kind(self, def_id: impl IntoQueryParam<DefId>) -> DefKind {
        let def_id = def_id.into_query_param();
        self.opt_def_kind(def_id)
            .unwrap_or_else(|| bug!("def_kind: unsupported node: {:?}", def_id))
    }
}
