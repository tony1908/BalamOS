use orbit_domain::{
    AgentEvent, BotSettings, GovernancePolicy, GovernancePreset, GovernanceRule, Routine,
    RuntimeSecret, SecretAssignment, SecretEnv, SecretInput, SecretMetadata, Skill, Template,
    Workspace, WorkspaceGovernancePolicy, builtin_governance_presets,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

pub trait CredentialStore: Send + Sync {
    fn set(&self, id: &str, value: &SecretInput) -> Result<(), StoreError>;
    fn get(&self, id: &str) -> Result<String, StoreError>;
    fn delete(&self, id: &str) -> Result<(), StoreError>;
}
pub struct KeyringCredentialStore;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl CredentialStore for KeyringCredentialStore {
    fn set(&self, id: &str, value: &SecretInput) -> Result<(), StoreError> {
        keyring::Entry::new("orbit-secret", id)
            .map_err(|_| StoreError::Credential)?
            .set_password(&value.0)
            .map_err(|_| StoreError::Credential)
    }
    fn get(&self, id: &str) -> Result<String, StoreError> {
        keyring::Entry::new("orbit-secret", id)
            .map_err(|_| StoreError::Credential)?
            .get_password()
            .map_err(|_| StoreError::Credential)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        match keyring::Entry::new("orbit-secret", id)
            .map_err(|_| StoreError::Credential)?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(StoreError::Credential),
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl CredentialStore for KeyringCredentialStore {
    fn set(&self, _id: &str, _value: &SecretInput) -> Result<(), StoreError> {
        Err(StoreError::Credential)
    }
    fn get(&self, _id: &str) -> Result<String, StoreError> {
        Err(StoreError::Credential)
    }
    fn delete(&self, _id: &str) -> Result<(), StoreError> {
        Err(StoreError::Credential)
    }
}

pub struct WorkspaceStore {
    connection: Mutex<Connection>,
    credentials: std::sync::Arc<dyn CredentialStore>,
    vault_lock: Arc<Mutex<()>>,
}

impl WorkspaceStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, StoreError> {
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS workspaces (
                 id TEXT PRIMARY KEY NOT NULL,
                 document TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS routines (
                 id TEXT PRIMARY KEY NOT NULL,
                 workspace_id TEXT NOT NULL,
                 document TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS skills (
                 id TEXT PRIMARY KEY NOT NULL,
                 workspace_id TEXT NOT NULL,
                 document TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS templates (
                 id TEXT PRIMARY KEY NOT NULL,
                 document TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS bot_settings (
                 workspace_id TEXT PRIMARY KEY NOT NULL,
                 document TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS runtime_secrets (
                 workspace_id TEXT PRIMARY KEY NOT NULL,
                 webtop_password TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS conversations (
                 workspace_id TEXT PRIMARY KEY NOT NULL,
                 events TEXT NOT NULL,
                 session_id TEXT
             );
             CREATE TABLE IF NOT EXISTS governance (
                  id INTEGER PRIMARY KEY CHECK (id = 1),
                  document TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS workspace_governance (
                  workspace_id TEXT PRIMARY KEY NOT NULL,
                   document TEXT NOT NULL
              );
               CREATE TABLE IF NOT EXISTS governance_presets (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    document TEXT NOT NULL
                );
               CREATE TABLE IF NOT EXISTS governance_rules (
                    id TEXT PRIMARY KEY NOT NULL,
                    title TEXT NOT NULL,
                    body TEXT NOT NULL
                );
                 CREATE TABLE IF NOT EXISTS secrets (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, env_name TEXT NOT NULL);
               CREATE TABLE IF NOT EXISTS secret_assignments (secret_id TEXT NOT NULL, workspace_id TEXT NOT NULL, agent TEXT NOT NULL, PRIMARY KEY(secret_id, workspace_id, agent));",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
            credentials: std::sync::Arc::new(KeyringCredentialStore),
            vault_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn with_credentials(mut self, credentials: std::sync::Arc<dyn CredentialStore>) -> Self {
        self.credentials = credentials;
        self
    }
    pub fn list_secrets(&self) -> Result<Vec<SecretMetadata>, StoreError> {
        let c = self.connection.lock().unwrap();
        let mut s = c.prepare("SELECT id,name,env_name FROM secrets ORDER BY name")?;
        Ok(s.query_map([], |r| {
            Ok(SecretMetadata {
                id: Uuid::parse_str(&r.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                name: r.get(1)?,
                env_name: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
    }
    fn valid_env(n: &str) -> bool {
        !n.is_empty()
            && n.len() <= 256
            && matches!(n.as_bytes()[0], b'A'..=b'Z' | b'_')
            && n.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && !matches!(
                n,
                "PATH"
                    | "HOME"
                    | "CODEX_HOME"
                    | "NODE_OPTIONS"
                    | "BASH_ENV"
                    | "ENV"
                    | "SHELLOPTS"
                    | "PYTHONPATH"
                    | "PYTHONHOME"
            )
            && !n.starts_with("LD_")
            && !n.starts_with("DYLD_")
            && !n.starts_with("ORBIT_")
    }
    pub fn create_secret(
        &self,
        name: String,
        env_name: String,
        value: SecretInput,
    ) -> Result<SecretMetadata, StoreError> {
        let _guard = self.vault_lock.lock().unwrap();
        let name = name.trim().to_string();
        if name.is_empty()
            || name.len() > 100
            || !Self::valid_env(&env_name)
            || value.0.is_empty()
            || value.0.len() > 65536
            || value.0.contains('\0')
        {
            return Err(StoreError::InvalidSecret);
        }
        let id = Uuid::new_v4();
        let m = SecretMetadata { id, name, env_name };
        self.credentials.set(&id.to_string(), &value)?;
        if let Err(e) = self.connection.lock().unwrap().execute(
            "INSERT INTO secrets(id,name,env_name) VALUES(?1,?2,?3)",
            params![id.to_string(), m.name, m.env_name],
        ) {
            let _ = self.credentials.delete(&id.to_string());
            return Err(e.into());
        }
        Ok(m)
    }
    pub fn replace_secret(&self, id: Uuid, value: SecretInput) -> Result<(), StoreError> {
        let _guard = self.vault_lock.lock().unwrap();
        if value.0.is_empty() || value.0.len() > 65536 || value.0.contains('\0') {
            return Err(StoreError::InvalidSecret);
        }
        if self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT id FROM secrets WHERE id=?1",
                params![id.to_string()],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_none()
        {
            return Err(StoreError::NotFound);
        }
        self.credentials.set(&id.to_string(), &value)
    }
    pub fn delete_secret(&self, id: Uuid) -> Result<(), StoreError> {
        let _guard = self.vault_lock.lock().unwrap();
        if self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT id FROM secrets WHERE id=?1",
                params![id.to_string()],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_none()
        {
            return Err(StoreError::NotFound);
        }
        self.credentials.delete(&id.to_string())?;
        let mut c = self.connection.lock().unwrap();
        let tx = c.transaction()?;
        tx.execute(
            "DELETE FROM secret_assignments WHERE secret_id=?1",
            params![id.to_string()],
        )?;
        tx.execute("DELETE FROM secrets WHERE id=?1", params![id.to_string()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn assign_secret(&self, a: SecretAssignment) -> Result<(), StoreError> {
        let _guard = self.vault_lock.lock().unwrap();
        let c = self.connection.lock().unwrap();
        if c.query_row(
            "SELECT id FROM secrets WHERE id=?1",
            params![a.secret_id.to_string()],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .is_none()
            || c.query_row(
                "SELECT id FROM workspaces WHERE id=?1",
                params![a.workspace_id.to_string()],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_none()
        {
            return Err(StoreError::NotFound);
        }
        if c.query_row("SELECT s.id FROM secrets s JOIN secret_assignments a ON a.secret_id=s.id WHERE a.workspace_id=?1 AND s.env_name=(SELECT env_name FROM secrets WHERE id=?2) AND s.id<>?2",params![a.workspace_id.to_string(),a.secret_id.to_string()],|r|r.get::<_,String>(0)).optional()?.is_some(){return Err(StoreError::DuplicateSecretEnv)}
        c.execute("INSERT OR REPLACE INTO secret_assignments(secret_id,workspace_id,agent) VALUES(?1,?2,'workspace')",params![a.secret_id.to_string(),a.workspace_id.to_string()])?;
        Ok(())
    }

    pub fn list_assignments(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<SecretAssignment>, StoreError> {
        let c = self.connection.lock().unwrap();
        let mut s = c.prepare(
            "SELECT secret_id, workspace_id FROM secret_assignments WHERE workspace_id=?1",
        )?;
        Ok(s.query_map(params![workspace_id.to_string()], |r| {
            Ok(SecretAssignment {
                secret_id: Uuid::parse_str(&r.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                workspace_id: Uuid::parse_str(&r.get::<_, String>(1)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn unassign_secret(&self, secret_id: Uuid, workspace_id: Uuid) -> Result<(), StoreError> {
        let _guard = self.vault_lock.lock().unwrap();
        self.connection.lock().unwrap().execute(
            "DELETE FROM secret_assignments WHERE secret_id=?1 AND workspace_id=?2",
            params![secret_id.to_string(), workspace_id.to_string()],
        )?;
        Ok(())
    }
    pub fn delete_assignments_for_workspace(&self, workspace_id: Uuid) -> Result<(), StoreError> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM secret_assignments WHERE workspace_id=?1",
            params![workspace_id.to_string()],
        )?;
        Ok(())
    }
    pub fn resolve_assigned_env(&self, workspace_id: Uuid) -> Result<Vec<SecretEnv>, StoreError> {
        let _guard = self.vault_lock.lock().unwrap();
        let c = self.connection.lock().unwrap();
        let mut s=c.prepare("SELECT s.env_name,s.id FROM secrets s JOIN secret_assignments a ON a.secret_id=s.id WHERE a.workspace_id=?1")?;
        let rows = s
            .query_map(params![workspace_id.to_string()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(s);
        rows.into_iter()
            .map(|(env, id)| {
                Ok(SecretEnv {
                    env_name: env,
                    value: SecretInput(self.credentials.get(&id)?),
                })
            })
            .collect()
    }

    pub fn list_governance_presets(&self) -> Result<Vec<GovernancePreset>, StoreError> {
        let mut presets = builtin_governance_presets();
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut statement = connection
            .prepare("SELECT id, name, document FROM governance_presets ORDER BY name")?;
        let custom = statement
            .query_map([], |row| {
                Ok(GovernancePreset {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    policy: serde_json::from_str(&row.get::<_, String>(2)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    builtin: false,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        presets.extend(custom);
        Ok(presets)
    }

    pub fn create_governance_preset(
        &self,
        name: String,
        policy: GovernancePolicy,
    ) -> Result<GovernancePreset, StoreError> {
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(StoreError::InvalidPresetName);
        }
        if policy.guidance.as_bytes().len() > 8000 {
            return Err(StoreError::GuidanceTooLong);
        }
        let preset = GovernancePreset {
            id: Uuid::new_v4().to_string(),
            name,
            policy,
            builtin: false,
        };
        let document = serde_json::to_string(&preset.policy)?;
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT INTO governance_presets (id, name, document) VALUES (?1, ?2, ?3)",
                params![preset.id, preset.name, document],
            )?;
        Ok(preset)
    }

    pub fn delete_governance_preset(&self, id: &str) -> Result<(), StoreError> {
        if builtin_governance_presets()
            .iter()
            .any(|preset| preset.id == id)
        {
            return Err(StoreError::BuiltinPreset);
        }
        if self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .execute("DELETE FROM governance_presets WHERE id=?1", params![id])?
            == 0
        {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn list_governance_rules(&self) -> Result<Vec<GovernanceRule>, StoreError> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut statement =
            connection.prepare("SELECT id, title, body FROM governance_rules ORDER BY title")?;
        let rules = statement
            .query_map([], |row| {
                Ok(GovernanceRule {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rules)
    }

    pub fn create_governance_rule(
        &self,
        title: String,
        body: String,
    ) -> Result<GovernanceRule, StoreError> {
        let title = title.trim().to_string();
        if title.is_empty() || title.chars().count() > 100 {
            return Err(StoreError::InvalidPresetName);
        }
        if body.as_bytes().len() > 8000 {
            return Err(StoreError::GuidanceTooLong);
        }
        let rule = GovernanceRule {
            id: Uuid::new_v4().to_string(),
            title,
            body,
        };
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT INTO governance_rules (id, title, body) VALUES (?1, ?2, ?3)",
                params![rule.id, rule.title, rule.body],
            )?;
        Ok(rule)
    }

    pub fn update_governance_rule(
        &self,
        id: &str,
        title: String,
        body: String,
    ) -> Result<GovernanceRule, StoreError> {
        let title = title.trim().to_string();
        if title.is_empty() || title.chars().count() > 100 {
            return Err(StoreError::InvalidPresetName);
        }
        if body.as_bytes().len() > 8000 {
            return Err(StoreError::GuidanceTooLong);
        }
        if self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "UPDATE governance_rules SET title=?2, body=?3 WHERE id=?1",
                params![id, title, body],
            )?
            == 0
        {
            return Err(StoreError::NotFound);
        }
        Ok(GovernanceRule {
            id: id.to_string(),
            title,
            body,
        })
    }

    pub fn delete_governance_rule(&self, id: &str) -> Result<(), StoreError> {
        if self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .execute("DELETE FROM governance_rules WHERE id=?1", params![id])?
            == 0
        {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn get_governance_policy(&self) -> Result<GovernancePolicy, StoreError> {
        let document: Option<String> = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row("SELECT document FROM governance WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        document
            .map(|d| Ok(serde_json::from_str(&d)?))
            .transpose()
            .map(|p| p.unwrap_or_default())
    }

    pub fn save_governance_policy(&self, policy: &GovernancePolicy) -> Result<(), StoreError> {
        if policy.guidance.as_bytes().len() > 8000 {
            return Err(StoreError::GuidanceTooLong);
        }
        let document = serde_json::to_string(policy)?;
        self.connection.lock().expect("store mutex poisoned").execute("INSERT INTO governance (id, document) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET document=excluded.document", params![document])?;
        Ok(())
    }

    pub fn get_workspace_governance(
        &self,
        id: Uuid,
    ) -> Result<WorkspaceGovernancePolicy, StoreError> {
        let document: Option<String> = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT document FROM workspace_governance WHERE workspace_id=?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        document
            .map(|d| Ok(serde_json::from_str(&d)?))
            .transpose()
            .map(|p| p.unwrap_or_default())
    }

    pub fn get_governance_pair(
        &self,
        id: Uuid,
    ) -> Result<(GovernancePolicy, WorkspaceGovernancePolicy), StoreError> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let global: Option<String> = connection
            .query_row("SELECT document FROM governance WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        let local: Option<String> = connection
            .query_row(
                "SELECT document FROM workspace_governance WHERE workspace_id=?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok((
            global
                .map(|d| serde_json::from_str(&d))
                .transpose()?
                .unwrap_or_default(),
            local
                .map(|d| serde_json::from_str(&d))
                .transpose()?
                .unwrap_or_default(),
        ))
    }

    pub fn save_workspace_governance(
        &self,
        id: Uuid,
        policy: &WorkspaceGovernancePolicy,
    ) -> Result<(), StoreError> {
        if self.get(id)?.is_none() {
            return Err(StoreError::NotFound);
        }
        if policy
            .guidance
            .as_ref()
            .is_some_and(|guidance| guidance.as_bytes().len() > 8000)
        {
            return Err(StoreError::GuidanceTooLong);
        }
        let document = serde_json::to_string(policy)?;
        self.connection.lock().expect("store mutex poisoned").execute("INSERT INTO workspace_governance (workspace_id, document) VALUES (?1, ?2) ON CONFLICT(workspace_id) DO UPDATE SET document=excluded.document", params![id.to_string(), document])?;
        Ok(())
    }

    pub fn upsert(&self, workspace: &Workspace) -> Result<(), StoreError> {
        let document = serde_json::to_string(workspace)?;
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT INTO workspaces (id, document) VALUES (?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET document=excluded.document",
                params![workspace.id.to_string(), document],
            )?;
        Ok(())
    }

    pub fn insert_workspace_with_secret(
        &self,
        workspace: &Workspace,
        secret: &RuntimeSecret,
    ) -> Result<(), StoreError> {
        if workspace.id != secret.workspace_id {
            return Err(StoreError::IdMismatch {
                workspace: workspace.id,
                secret: secret.workspace_id,
            });
        }
        let document = serde_json::to_string(workspace)?;
        let mut connection = self.connection.lock().expect("store mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO workspaces (id, document) VALUES (?1, ?2)",
            params![workspace.id.to_string(), document],
        )?;
        transaction.execute(
            "INSERT INTO runtime_secrets (workspace_id, webtop_password) VALUES (?1, ?2)",
            params![secret.workspace_id.to_string(), secret.webtop_password],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get(&self, id: Uuid) -> Result<Option<Workspace>, StoreError> {
        let document = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT document FROM workspaces WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        document
            .map(|document| Ok(serde_json::from_str(&document)?))
            .transpose()
    }

    pub fn list(&self) -> Result<Vec<Workspace>, StoreError> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut statement = connection.prepare("SELECT document FROM workspaces ORDER BY id")?;
        let documents = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        documents
            .into_iter()
            .map(|document| Ok(serde_json::from_str(&document)?))
            .collect()
    }

    pub fn compare_and_swap(
        &self,
        expected: &Workspace,
        replacement: &Workspace,
    ) -> Result<bool, StoreError> {
        let new = serde_json::to_string(replacement)?;
        let connection = self.connection.lock().expect("store mutex poisoned");
        let current: Option<String> = connection
            .query_row(
                "SELECT document FROM workspaces WHERE id = ?1",
                params![expected.id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            return Ok(false);
        };
        let current_workspace: Workspace = serde_json::from_str(&current)?;
        if &current_workspace != expected {
            return Ok(false);
        }
        let changed = connection.execute(
            "UPDATE workspaces SET document = ?1 WHERE id = ?2 AND document = ?3",
            params![new, expected.id.to_string(), current],
        )?;
        Ok(changed == 1)
    }

    pub fn put_runtime_secret(&self, secret: &RuntimeSecret) -> Result<(), StoreError> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT INTO runtime_secrets (workspace_id, webtop_password) VALUES (?1, ?2)
             ON CONFLICT(workspace_id) DO UPDATE SET webtop_password=excluded.webtop_password",
                params![secret.workspace_id.to_string(), secret.webtop_password],
            )?;
        Ok(())
    }

    pub fn get_runtime_secret(&self, id: Uuid) -> Result<Option<RuntimeSecret>, StoreError> {
        let password = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT webtop_password FROM runtime_secrets WHERE workspace_id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(password.map(|webtop_password| RuntimeSecret {
            workspace_id: id,
            webtop_password,
        }))
    }

    pub fn delete_runtime_secret(&self, id: Uuid) -> Result<(), StoreError> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "DELETE FROM runtime_secrets WHERE workspace_id = ?1",
                params![id.to_string()],
            )?;
        Ok(())
    }

    pub fn delete_workspace(&self, id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM runtime_secrets WHERE workspace_id = ?1",
            params![id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM workspaces WHERE id = ?1",
            params![id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM secret_assignments WHERE workspace_id = ?1",
            params![id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM workspace_governance WHERE workspace_id = ?1",
            params![id.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn upsert_routine(&self, routine: &Routine) -> Result<(), StoreError> {
        let document = serde_json::to_string(routine)?;
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT INTO routines (id, workspace_id, document) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET workspace_id=excluded.workspace_id, document=excluded.document",
                params![routine.id.to_string(), routine.workspace_id.to_string(), document],
            )?;
        Ok(())
    }

    pub fn upsert_template(&self, template: &Template) -> Result<(), StoreError> {
        let document = serde_json::to_string(template)?;
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT OR REPLACE INTO templates (id, document) VALUES (?1, ?2)",
                params![template.id.to_string(), document],
            )?;
        Ok(())
    }

    pub fn get_template(&self, id: Uuid) -> Result<Option<Template>, StoreError> {
        let document = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT document FROM templates WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        document
            .map(|document| Ok(serde_json::from_str(&document)?))
            .transpose()
    }

    pub fn list_all_templates(&self) -> Result<Vec<Template>, StoreError> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut statement = connection.prepare("SELECT document FROM templates")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|document| Ok(serde_json::from_str(&document)?))
            .collect()
    }

    pub fn delete_template(&self, id: Uuid) -> Result<(), StoreError> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "DELETE FROM templates WHERE id = ?1",
                params![id.to_string()],
            )?;
        Ok(())
    }

    pub fn get_routine(&self, id: Uuid) -> Result<Option<Routine>, StoreError> {
        let document = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT document FROM routines WHERE id=?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        document
            .map(|document| Ok(serde_json::from_str(&document)?))
            .transpose()
    }

    pub fn list_routines(&self, workspace_id: Uuid) -> Result<Vec<Routine>, StoreError> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut statement =
            connection.prepare("SELECT document FROM routines WHERE workspace_id=?1")?;
        let documents = statement
            .query_map(params![workspace_id.to_string()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        documents
            .into_iter()
            .map(|document| Ok(serde_json::from_str(&document)?))
            .collect()
    }

    pub fn list_all_routines(&self) -> Result<Vec<Routine>, StoreError> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut statement = connection.prepare("SELECT document FROM routines")?;
        let documents = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        documents
            .into_iter()
            .map(|document| Ok(serde_json::from_str(&document)?))
            .collect()
    }

    pub fn delete_routine(&self, id: Uuid) -> Result<(), StoreError> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute("DELETE FROM routines WHERE id=?1", params![id.to_string()])?;
        Ok(())
    }

    pub fn delete_routines_for_workspace(&self, workspace_id: Uuid) -> Result<(), StoreError> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "DELETE FROM routines WHERE workspace_id=?1",
                params![workspace_id.to_string()],
            )?;
        Ok(())
    }

    pub fn upsert_skill(&self, skill: &Skill) -> Result<(), StoreError> {
        let document = serde_json::to_string(skill)?;
        self.connection.lock().unwrap().execute(
            "INSERT OR REPLACE INTO skills (id, workspace_id, document) VALUES (?1, ?2, ?3)",
            params![
                skill.id.to_string(),
                skill.workspace_id.to_string(),
                document
            ],
        )?;
        Ok(())
    }
    pub fn get_skill(&self, id: Uuid) -> Result<Option<Skill>, StoreError> {
        let connection = self.connection.lock().unwrap();
        let document = connection
            .query_row(
                "SELECT document FROM skills WHERE id=?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        document
            .map(|d| serde_json::from_str(&d))
            .transpose()
            .map_err(Into::into)
    }
    pub fn list_skills(&self, workspace_id: Uuid) -> Result<Vec<Skill>, StoreError> {
        let connection = self.connection.lock().unwrap();
        let mut statement =
            connection.prepare("SELECT document FROM skills WHERE workspace_id=?1")?;
        let rows = statement.query_map(params![workspace_id.to_string()], |row| {
            row.get::<_, String>(0)
        })?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn list_all_skills(&self) -> Result<Vec<Skill>, StoreError> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection.prepare("SELECT document FROM skills")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn delete_skill(&self, id: Uuid) -> Result<(), StoreError> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM skills WHERE id=?1", params![id.to_string()])?;
        Ok(())
    }
    pub fn delete_skills_for_workspace(&self, workspace_id: Uuid) -> Result<(), StoreError> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM skills WHERE workspace_id=?1",
            params![workspace_id.to_string()],
        )?;
        Ok(())
    }

    pub fn upsert_bot_settings(&self, settings: &BotSettings) -> Result<(), StoreError> {
        let document = serde_json::to_string(settings)?;
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT OR REPLACE INTO bot_settings (workspace_id, document) VALUES (?1, ?2)",
                params![settings.workspace_id.to_string(), document],
            )?;
        Ok(())
    }

    pub fn get_bot_settings(&self, workspace_id: Uuid) -> Result<Option<BotSettings>, StoreError> {
        let document = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT document FROM bot_settings WHERE workspace_id=?1",
                params![workspace_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        document
            .map(|document| Ok(serde_json::from_str(&document)?))
            .transpose()
    }

    pub fn list_all_bot_settings(&self) -> Result<Vec<BotSettings>, StoreError> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut statement = connection.prepare("SELECT document FROM bot_settings")?;
        let documents = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        documents
            .into_iter()
            .map(|document| Ok(serde_json::from_str(&document)?))
            .collect()
    }

    pub fn delete_bot_settings_for_workspace(&self, workspace_id: Uuid) -> Result<(), StoreError> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "DELETE FROM bot_settings WHERE workspace_id=?1",
                params![workspace_id.to_string()],
            )?;
        Ok(())
    }

    pub fn save_conversation(
        &self,
        workspace_id: Uuid,
        events: &[AgentEvent],
        session_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let json = serde_json::to_string(events)?;
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "INSERT INTO conversations (workspace_id, events, session_id) VALUES (?1, ?2, ?3)
                 ON CONFLICT(workspace_id) DO UPDATE SET events=excluded.events, session_id=excluded.session_id",
                params![workspace_id.to_string(), json, session_id],
            )?;
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    pub fn load_conversation(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<(Vec<AgentEvent>, Option<String>)>, StoreError> {
        let row = self
            .connection
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT events, session_id FROM conversations WHERE workspace_id=?1",
                params![workspace_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        row.map(|(events, session_id)| Ok((serde_json::from_str(&events)?, session_id)))
            .transpose()
    }

    pub fn delete_conversation(&self, workspace_id: Uuid) -> Result<(), StoreError> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "DELETE FROM conversations WHERE workspace_id=?1",
                params![workspace_id.to_string()],
            )?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("workspace not found")]
    NotFound,
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("workspace serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("workspace and runtime secret IDs do not match ({workspace} != {secret})")]
    IdMismatch { workspace: Uuid, secret: Uuid },
    #[error("governance guidance exceeds 8000 bytes")]
    GuidanceTooLong,
    #[error("preset name must be 1-100 characters")]
    InvalidPresetName,
    #[error("built-in governance presets cannot be deleted")]
    BuiltinPreset,
    #[error("credential store unavailable")]
    Credential,
    #[error("invalid secret")]
    InvalidSecret,
    #[error("a secret with this environment variable is already assigned to the workspace")]
    DuplicateSecretEnv,
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_domain::{AgentEventKind, AgentSessionState};

    #[test]
    fn conversation_round_trip_and_delete() {
        let store = WorkspaceStore::in_memory().unwrap();
        let workspace_id = Uuid::new_v4();
        let events = vec![
            AgentEvent {
                cursor: 1,
                kind: AgentEventKind::StateChanged {
                    state: AgentSessionState::Running,
                },
            },
            AgentEvent {
                cursor: 2,
                kind: AgentEventKind::AssistantDelta {
                    text: "hello".into(),
                },
            },
        ];

        store
            .save_conversation(workspace_id, &events, Some("session-1"))
            .unwrap();
        assert_eq!(
            store.load_conversation(workspace_id).unwrap(),
            Some((events, Some("session-1".into())))
        );
        store.delete_conversation(workspace_id).unwrap();
        assert_eq!(store.load_conversation(workspace_id).unwrap(), None);
    }

    #[test]
    fn deleting_workspace_removes_secret_assignments() {
        let store = WorkspaceStore::in_memory().unwrap();
        let workspace = Workspace::new(
            "workspace".into(),
            std::env::temp_dir(),
            orbit_domain::PermissionProfile::Workspace,
            orbit_domain::ResourceLimits::new(1.0, 1, 1, 1).unwrap(),
        );
        let secret = SecretMetadata {
            id: Uuid::new_v4(),
            name: "token".into(),
            env_name: "TOKEN".into(),
        };
        store.upsert(&workspace).unwrap();
        let secret = store
            .create_secret(
                secret.name.clone(),
                secret.env_name.clone(),
                SecretInput("value".into()),
            )
            .unwrap();
        store
            .assign_secret(SecretAssignment {
                secret_id: secret.id,
                workspace_id: workspace.id,
            })
            .unwrap();
        store.delete_workspace(workspace.id).unwrap();
        assert!(store.list_assignments(workspace.id).unwrap().is_empty());
    }

    #[test]
    fn list_all_routines_returns_routines_across_workspaces() {
        let store = WorkspaceStore::in_memory().unwrap();
        let make_routine = |name: &str, workspace_id| Routine {
            id: Uuid::new_v4(),
            workspace_id,
            name: name.into(),
            instruction: "run".into(),
            interval_minutes: 1,
            enabled: true,
            last_run_unix_ms: None,
            last_status: None,
            last_skip_reason: None,
        };
        let first = make_routine("first", Uuid::new_v4());
        let second = make_routine("second", Uuid::new_v4());
        store.upsert_routine(&first).unwrap();
        store.upsert_routine(&second).unwrap();

        let routines = store.list_all_routines().unwrap();

        assert_eq!(routines.len(), 2);
        assert!(routines.contains(&first));
        assert!(routines.contains(&second));
    }

    #[test]
    fn skills_round_trip_list_and_delete() {
        let store = WorkspaceStore::in_memory().unwrap();
        let first = Skill {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: "first".into(),
            instruction: "do first".into(),
            enabled: true,
        };
        let second = Skill {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: "second".into(),
            instruction: "do second".into(),
            enabled: false,
        };
        store.upsert_skill(&first).unwrap();
        store.upsert_skill(&second).unwrap();
        assert_eq!(store.get_skill(first.id).unwrap(), Some(first.clone()));
        assert_eq!(
            store.list_skills(first.workspace_id).unwrap(),
            vec![first.clone()]
        );
        assert_eq!(store.list_all_skills().unwrap().len(), 2);
        store.delete_skill(first.id).unwrap();
        assert_eq!(store.get_skill(first.id).unwrap(), None);
        store
            .delete_skills_for_workspace(second.workspace_id)
            .unwrap();
        assert!(store.list_all_skills().unwrap().is_empty());
    }

    #[test]
    fn bot_settings_round_trip_list_and_delete() {
        let store = WorkspaceStore::in_memory().unwrap();
        let first = BotSettings {
            workspace_id: Uuid::new_v4(),
            display_name: Some("Bot".into()),
            label: None,
            description: Some("desc".into()),
            avatar_color: None,
            notifications: true,
        };
        let second = BotSettings {
            workspace_id: Uuid::new_v4(),
            display_name: None,
            label: Some("label".into()),
            description: None,
            avatar_color: None,
            notifications: false,
        };
        assert_eq!(store.get_bot_settings(first.workspace_id).unwrap(), None);
        store.upsert_bot_settings(&first).unwrap();
        store.upsert_bot_settings(&second).unwrap();
        assert_eq!(
            store.get_bot_settings(first.workspace_id).unwrap(),
            Some(first.clone())
        );
        assert_eq!(store.list_all_bot_settings().unwrap().len(), 2);
        store
            .delete_bot_settings_for_workspace(first.workspace_id)
            .unwrap();
        assert_eq!(store.get_bot_settings(first.workspace_id).unwrap(), None);
    }

    #[test]
    fn templates_round_trip_list_and_delete() {
        let store = WorkspaceStore::in_memory().unwrap();
        let template = Template {
            id: Uuid::new_v4(),
            name: "first".into(),
            settings: orbit_domain::TemplateSettings {
                display_name: None,
                label: None,
                description: None,
                notifications: true,
            },
            routines: vec![],
            skills: vec![],
        };
        let second = Template {
            id: Uuid::new_v4(),
            name: "second".into(),
            ..template.clone()
        };
        store.upsert_template(&template).unwrap();
        store.upsert_template(&second).unwrap();
        assert_eq!(
            store.get_template(template.id).unwrap(),
            Some(template.clone())
        );
        assert_eq!(store.list_all_templates().unwrap().len(), 2);
        store.delete_template(template.id).unwrap();
        assert_eq!(store.get_template(template.id).unwrap(), None);
    }
}
