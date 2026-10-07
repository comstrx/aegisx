use std::collections::HashMap;
use super::RouteState;

/// Immutable path candidates preserve the existing global host/method/header priority.
pub(super) struct RouteIndex { paths: HashMap<String,Vec<usize>> }
impl RouteIndex {
    pub fn build ( routes: &[std::sync::Arc<RouteState>] ) -> Self {
        let mut paths:HashMap<String,Vec<usize>>=HashMap::new();
        for (rank,route) in routes.iter().enumerate() {paths.entry(route.spec.path.clone()).or_default().push(rank);}
        Self {paths}
    }
    pub fn select ( &self, path: &str, mut matches: impl FnMut(usize)->bool ) -> Option<usize> {
        let mut best=None;
        let mut visit=|key:&str| {
            if let Some(ranks)=self.paths.get(key) {
                for rank in ranks {
                    if best.is_some_and(|best|*rank>=best) {break;}
                    if matches(*rank) {best=Some(*rank);break;}
                }
            }
        };
        // Caller additionally enforces exact paths; these are only candidate boundaries.
        visit(path);
        for offset in memchr::memchr_iter(b'/',path.as_bytes()) {
            if offset>0 {visit(&path[..offset]);}
            if offset+1<path.len() {visit(&path[..offset+1]);}
        }
        best
    }
}
