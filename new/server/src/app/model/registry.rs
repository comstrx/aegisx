use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::config::Config;
use crate::core::error::{AppError, AppResult};
use super::arch::{Model, Models, REGISTRY};

impl Models {

    pub fn install ( config: &Config ) -> AppResult<()> {

        let mut loaded = HashMap::with_capacity(config.models.len());

        for ( name, spec ) in &config.models {

            loaded.insert(Arc::from(name.as_str()), Arc::new(Model::load(name, &spec.dir, spec.features.as_deref(), spec.threads)?));

        }

        let registry = REGISTRY.get_or_init(|| RwLock::new(HashMap::new()));

        *registry.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = loaded;

        Ok(())

    }

    pub fn names () -> Vec<Arc<str>> {

        REGISTRY.get().map(|registry| registry.read().unwrap_or_else(|poisoned| poisoned.into_inner()).keys().cloned().collect()).unwrap_or_default()

    }

}

impl Model {

    pub fn named ( name: &str ) -> AppResult<Arc<Model>> {

        REGISTRY.get()
            .and_then(|registry| registry.read().unwrap_or_else(|poisoned| poisoned.into_inner()).get(name).cloned())
            .ok_or_else(|| AppError::not_found(format!("model `{name}`")))

    }

}
