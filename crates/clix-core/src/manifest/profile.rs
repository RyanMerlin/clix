use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_profile_yaml() {
        let yaml = "name: kubectl-observe\nversion: 1\ncapabilities: [kubectl.get-pods]\n";
        let p: ProfileManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.name, "kubectl-observe");
        assert_eq!(p.capabilities.len(), 1);
    }
}
