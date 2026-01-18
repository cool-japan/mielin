//! Agent Templates
//!
//! Provides reusable agent configuration templates for quickly
//! instantiating agents with predefined settings, policies, and DNA.

use crate::{Agent, Policy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Unique identifier for templates
pub type TemplateId = Uuid;

/// Agent template for creating agents with predefined configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    /// Unique template identifier
    pub id: TemplateId,
    /// Template name
    pub name: String,
    /// Template description
    pub description: String,
    /// WASM binary DNA
    pub dna_binary: Vec<u8>,
    /// Default policy
    pub policy: Policy,
    /// Template metadata
    pub metadata: HashMap<String, String>,
    /// Template version
    pub version: String,
    /// Template tags for categorization
    pub tags: Vec<String>,
    /// Created timestamp
    pub created_at: u64,
}

impl AgentTemplate {
    /// Create a new template
    pub fn new(name: impl Into<String>, dna_binary: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            dna_binary,
            policy: Policy::default(),
            metadata: HashMap::new(),
            version: "1.0.0".to_string(),
            tags: Vec::new(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Create template with builder pattern
    pub fn builder(name: impl Into<String>, dna_binary: Vec<u8>) -> TemplateBuilder {
        TemplateBuilder::new(name, dna_binary)
    }

    /// Instantiate an agent from this template
    pub fn instantiate(&self) -> Agent {
        let mut agent = Agent::new(self.dna_binary.clone());
        agent.set_policy(self.policy.clone());
        agent
    }

    /// Instantiate an agent with policy override
    pub fn instantiate_with_policy(&self, policy: Policy) -> Agent {
        let mut agent = Agent::new(self.dna_binary.clone());
        agent.set_policy(policy);
        agent
    }

    /// Get template ID
    pub fn id(&self) -> TemplateId {
        self.id
    }

    /// Get template name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get template description
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Get template version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Check if template has a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Get all tags
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Clone template with new ID (template versioning)
    pub fn clone_as_new_version(&self, version: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            version: version.into(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            ..self.clone()
        }
    }
}

/// Builder for creating agent templates
pub struct TemplateBuilder {
    template: AgentTemplate,
}

impl TemplateBuilder {
    /// Create a new template builder
    pub fn new(name: impl Into<String>, dna_binary: Vec<u8>) -> Self {
        Self {
            template: AgentTemplate::new(name, dna_binary),
        }
    }

    /// Set template description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.template.description = description.into();
        self
    }

    /// Set template policy
    pub fn policy(mut self, policy: Policy) -> Self {
        self.template.policy = policy;
        self
    }

    /// Set template version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.template.version = version.into();
        self
    }

    /// Add a tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.template.tags.push(tag.into());
        self
    }

    /// Add multiple tags
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.template.tags.extend(tags);
        self
    }

    /// Add metadata
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.template.metadata.insert(key.into(), value.into());
        self
    }

    /// Build the template
    pub fn build(self) -> AgentTemplate {
        self.template
    }
}

/// Registry for managing agent templates
pub struct TemplateRegistry {
    /// All registered templates
    templates: RwLock<HashMap<TemplateId, Arc<AgentTemplate>>>,
    /// Index: name -> template ID (latest version)
    name_index: RwLock<HashMap<String, TemplateId>>,
    /// Index: tag -> template IDs
    tag_index: RwLock<HashMap<String, Vec<TemplateId>>>,
}

impl TemplateRegistry {
    /// Create a new template registry
    pub fn new() -> Self {
        Self {
            templates: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
            tag_index: RwLock::new(HashMap::new()),
        }
    }

    /// Register a template
    pub fn register(&self, template: AgentTemplate) -> Arc<AgentTemplate> {
        let id = template.id();
        let name = template.name().to_string();
        let tags = template.tags().to_vec();

        let template = Arc::new(template);

        // Store template
        self.templates
            .write()
            .expect("Lock poisoned: templates")
            .insert(id, Arc::clone(&template));

        // Update name index (latest version)
        self.name_index
            .write()
            .expect("Lock poisoned: name_index")
            .insert(name, id);

        // Update tag index
        let mut tag_idx = self.tag_index.write().expect("Lock poisoned: tag_index");
        for tag in tags {
            tag_idx.entry(tag).or_default().push(id);
        }

        template
    }

    /// Get template by ID
    pub fn get(&self, id: &TemplateId) -> Option<Arc<AgentTemplate>> {
        self.templates
            .read()
            .expect("Lock poisoned: templates")
            .get(id)
            .cloned()
    }

    /// Get template by name (returns latest version)
    pub fn get_by_name(&self, name: &str) -> Option<Arc<AgentTemplate>> {
        let name_idx = self.name_index.read().expect("Lock poisoned: name_index");
        let id = name_idx.get(name)?;
        self.get(id)
    }

    /// Get all templates with a tag
    pub fn get_by_tag(&self, tag: &str) -> Vec<Arc<AgentTemplate>> {
        let tag_idx = self.tag_index.read().expect("Lock poisoned: tag_index");
        let templates = self.templates.read().expect("Lock poisoned: templates");

        tag_idx
            .get(tag)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| templates.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Unregister a template
    pub fn unregister(&self, id: &TemplateId) -> Option<Arc<AgentTemplate>> {
        let template = self
            .templates
            .write()
            .expect("Lock poisoned: templates")
            .remove(id)?;

        // Remove from name index if it's the latest
        let mut name_idx = self.name_index.write().expect("Lock poisoned: name_index");
        if name_idx.get(template.name()) == Some(id) {
            name_idx.remove(template.name());
        }

        // Remove from tag index
        let mut tag_idx = self.tag_index.write().expect("Lock poisoned: tag_index");
        for tag in template.tags() {
            if let Some(ids) = tag_idx.get_mut(tag) {
                ids.retain(|tid| tid != id);
            }
        }

        Some(template)
    }

    /// Get total number of registered templates
    pub fn count(&self) -> usize {
        self.templates
            .read()
            .expect("Lock poisoned: templates")
            .len()
    }

    /// Get all template IDs
    pub fn all_ids(&self) -> Vec<TemplateId> {
        self.templates
            .read()
            .expect("Lock poisoned: templates")
            .keys()
            .copied()
            .collect()
    }

    /// Get all templates
    pub fn all(&self) -> Vec<Arc<AgentTemplate>> {
        self.templates
            .read()
            .expect("Lock poisoned: templates")
            .values()
            .cloned()
            .collect()
    }

    /// Instantiate an agent from a template by ID
    pub fn instantiate(&self, id: &TemplateId) -> Result<Agent, TemplateError> {
        let template = self.get(id).ok_or(TemplateError::TemplateNotFound(*id))?;
        Ok(template.instantiate())
    }

    /// Instantiate an agent from a template by name
    pub fn instantiate_by_name(&self, name: &str) -> Result<Agent, TemplateError> {
        let template = self
            .get_by_name(name)
            .ok_or_else(|| TemplateError::TemplateNotFoundByName(name.to_string()))?;
        Ok(template.instantiate())
    }

    /// Instantiate multiple agents from a template
    pub fn instantiate_many(
        &self,
        id: &TemplateId,
        count: usize,
    ) -> Result<Vec<Agent>, TemplateError> {
        let template = self.get(id).ok_or(TemplateError::TemplateNotFound(*id))?;

        let agents = (0..count).map(|_| template.instantiate()).collect();
        Ok(agents)
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Template-related errors
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    TemplateNotFound(TemplateId),

    #[error("Template not found by name: {0}")]
    TemplateNotFoundByName(String),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Agent instantiation failed: {0}")]
    InstantiationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_creation() {
        let template = AgentTemplate::new("web-server", vec![0x00, 0x61, 0x73, 0x6d]);
        assert_eq!(template.name(), "web-server");
        assert_eq!(template.version(), "1.0.0");
    }

    #[test]
    fn test_template_builder() {
        let template = AgentTemplate::builder("api-server", vec![0x00, 0x61, 0x73, 0x6d])
            .description("REST API server")
            .version("2.0.0")
            .tag("production")
            .tag("api")
            .metadata("region", "us-west-2")
            .metadata("tier", "premium")
            .build();

        assert_eq!(template.name(), "api-server");
        assert_eq!(template.description(), "REST API server");
        assert_eq!(template.version(), "2.0.0");
        assert!(template.has_tag("production"));
        assert!(template.has_tag("api"));
        assert_eq!(
            template.get_metadata("region"),
            Some(&"us-west-2".to_string())
        );
    }

    #[test]
    fn test_template_instantiate() {
        let template = AgentTemplate::new("worker", vec![0x00, 0x61, 0x73, 0x6d]);
        let agent = template.instantiate();

        assert_ne!(agent.id(), Uuid::nil());
    }

    #[test]
    fn test_template_clone_version() {
        let template = AgentTemplate::new("service", vec![0x00, 0x61, 0x73, 0x6d]);
        let original_id = template.id();

        let v2 = template.clone_as_new_version("2.0.0");

        assert_ne!(v2.id(), original_id);
        assert_eq!(v2.version(), "2.0.0");
        assert_eq!(v2.name(), template.name());
    }

    #[test]
    fn test_registry_register() {
        let registry = TemplateRegistry::new();
        let template = AgentTemplate::new("test", vec![0x00, 0x61, 0x73, 0x6d]);
        let id = template.id();

        registry.register(template);

        assert_eq!(registry.count(), 1);
        assert!(registry.get(&id).is_some());
    }

    #[test]
    fn test_registry_get_by_name() {
        let registry = TemplateRegistry::new();
        let template = AgentTemplate::new("my-service", vec![0x00, 0x61, 0x73, 0x6d]);

        registry.register(template);

        let retrieved = registry.get_by_name("my-service");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "my-service");
    }

    #[test]
    fn test_registry_get_by_tag() {
        let registry = TemplateRegistry::new();

        let t1 = AgentTemplate::builder("svc1", vec![0x00, 0x61, 0x73, 0x6d])
            .tag("production")
            .build();

        let t2 = AgentTemplate::builder("svc2", vec![0x00, 0x61, 0x73, 0x6d])
            .tag("production")
            .tag("critical")
            .build();

        let t3 = AgentTemplate::builder("svc3", vec![0x00, 0x61, 0x73, 0x6d])
            .tag("development")
            .build();

        registry.register(t1);
        registry.register(t2);
        registry.register(t3);

        let prod_templates = registry.get_by_tag("production");
        assert_eq!(prod_templates.len(), 2);

        let dev_templates = registry.get_by_tag("development");
        assert_eq!(dev_templates.len(), 1);
    }

    #[test]
    fn test_registry_unregister() {
        let registry = TemplateRegistry::new();
        let template = AgentTemplate::new("temp", vec![0x00, 0x61, 0x73, 0x6d]);
        let id = template.id();

        registry.register(template);
        assert_eq!(registry.count(), 1);

        let removed = registry.unregister(&id);
        assert!(removed.is_some());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_registry_instantiate() {
        let registry = TemplateRegistry::new();
        let template = AgentTemplate::new("worker", vec![0x00, 0x61, 0x73, 0x6d]);
        let id = template.id();

        registry.register(template);

        let agent = registry.instantiate(&id);
        assert!(agent.is_ok());
    }

    #[test]
    fn test_registry_instantiate_by_name() {
        let registry = TemplateRegistry::new();
        let template = AgentTemplate::new("my-worker", vec![0x00, 0x61, 0x73, 0x6d]);

        registry.register(template);

        let agent = registry.instantiate_by_name("my-worker");
        assert!(agent.is_ok());
    }

    #[test]
    fn test_registry_instantiate_many() {
        let registry = TemplateRegistry::new();
        let template = AgentTemplate::new("pool-worker", vec![0x00, 0x61, 0x73, 0x6d]);
        let id = template.id();

        registry.register(template);

        let agents = registry.instantiate_many(&id, 5);
        assert!(agents.is_ok());
        assert_eq!(agents.unwrap().len(), 5);
    }

    #[test]
    fn test_template_tags() {
        let template = AgentTemplate::builder("service", vec![0x00, 0x61, 0x73, 0x6d])
            .tags(vec!["prod".to_string(), "api".to_string()])
            .tag("v2")
            .build();

        assert_eq!(template.tags().len(), 3);
        assert!(template.has_tag("prod"));
        assert!(template.has_tag("api"));
        assert!(template.has_tag("v2"));
        assert!(!template.has_tag("dev"));
    }

    #[test]
    fn test_registry_all() {
        let registry = TemplateRegistry::new();

        registry.register(AgentTemplate::new("t1", vec![0x00, 0x61, 0x73, 0x6d]));
        registry.register(AgentTemplate::new("t2", vec![0x00, 0x61, 0x73, 0x6d]));
        registry.register(AgentTemplate::new("t3", vec![0x00, 0x61, 0x73, 0x6d]));

        let all = registry.all();
        assert_eq!(all.len(), 3);
    }
}
