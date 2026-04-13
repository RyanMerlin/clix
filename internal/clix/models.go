package clix

// CredentialSource describes a single credential to inject into a subprocess environment.
type CredentialSource struct {
	// InjectAs is the env var name set in the subprocess (e.g. "AWS_ACCESS_KEY_ID").
	InjectAs string `json:"injectAs"`
	// Type is one of: "env", "literal", "infisical".
	Type string `json:"type"`
	// EnvVar is the source env var name when Type is "env".
	EnvVar string `json:"envVar,omitempty"`
	// Value is the literal credential value when Type is "literal" (dev/test only).
	Value string `json:"value,omitempty"`
	// Infisical specifies a secret reference when Type is "infisical".
	Infisical *InfisicalRef `json:"infisical,omitempty"`
}

// InfisicalRef identifies a secret in Infisical.
type InfisicalRef struct {
	SecretName  string `json:"secretName"`
	ProjectID   string `json:"projectId,omitempty"`
	Environment string `json:"environment"`
	SecretPath  string `json:"secretPath,omitempty"` // defaults to "/"
}

type CapabilityBackend struct {
	Type         string             `json:"type"`
	Name         string             `json:"name,omitempty"`
	Command      string             `json:"command,omitempty"`
	Args         []string           `json:"args,omitempty"`
	CwdFromInput string             `json:"cwdFromInput,omitempty"`
	URL          string             `json:"url,omitempty"`
	Credentials  []CredentialSource `json:"credentials,omitempty"`
}

type Schema map[string]any

type CapabilityManifest struct {
	Name            string            `json:"name"`
	Version         int               `json:"version"`
	Description     string            `json:"description,omitempty"`
	Backend         CapabilityBackend `json:"backend"`
	Risk            string            `json:"risk,omitempty"`
	SideEffectClass string            `json:"sideEffectClass,omitempty"`
	SandboxProfile  string            `json:"sandboxProfile,omitempty"`
	ApprovalPolicy  string            `json:"approvalPolicy,omitempty"`
	InputSchema     map[string]any    `json:"inputSchema,omitempty"`
	OutputSchema    map[string]any    `json:"outputSchema,omitempty"`
	Validators      []Validator       `json:"validators,omitempty"`
}

type WorkflowStep struct {
	Name       string         `json:"name"`
	Capability string         `json:"capability"`
	Input      map[string]any `json:"input,omitempty"`
}

type WorkflowManifest struct {
	Name        string         `json:"name"`
	Version     int            `json:"version"`
	Description string         `json:"description,omitempty"`
	InputSchema map[string]any `json:"inputSchema,omitempty"`
	Steps       []WorkflowStep `json:"steps,omitempty"`
}

type PolicyBundle struct {
	SchemaVersion   int          `json:"schemaVersion"`
	DefaultDecision string       `json:"defaultDecision"`
	Rules           []PolicyRule `json:"rules"`
}

type PolicyRule struct {
	Effect string      `json:"effect"`
	Match  PolicyMatch `json:"match"`
	Reason string      `json:"reason,omitempty"`
}

type PolicyMatch struct {
	Capabilities []string `json:"capabilities,omitempty"`
	Profiles     []string `json:"profiles,omitempty"`
	Envs         []string `json:"envs,omitempty"`
	Risk         []string `json:"risk,omitempty"`
	SideEffects  []string `json:"sideEffects,omitempty"`
	Backends     []string `json:"backends,omitempty"`
}

type Validator struct {
	Type   string   `json:"type"`
	Path   string   `json:"path,omitempty"`
	Values []string `json:"values,omitempty"`
	Key    string   `json:"key,omitempty"`
}

type ProfileManifest struct {
	Name         string               `json:"name"`
	Version      int                  `json:"version"`
	Description  string               `json:"description,omitempty"`
	Imports      []string             `json:"imports,omitempty"`
	Capabilities []CapabilityManifest `json:"capabilities,omitempty"`
	Workflows    []WorkflowManifest   `json:"workflows,omitempty"`
	Policy       *PolicyBundle        `json:"policy,omitempty"`
	Settings     map[string]any       `json:"settings,omitempty"`
}

type PackManifest struct {
	Name         string   `json:"name"`
	Version      int      `json:"version"`
	Description  string   `json:"description,omitempty"`
	Profiles     []string `json:"profiles,omitempty"`
	Capabilities []string `json:"capabilities,omitempty"`
	Workflows    []string `json:"workflows,omitempty"`
	Plugins      []string `json:"plugins,omitempty"`
}

type PackBundleFile struct {
	Path   string `json:"path"`
	Size   int64  `json:"size"`
	SHA256 string `json:"sha256"`
}

type PackBundleManifest struct {
	SchemaVersion int              `json:"schemaVersion"`
	CreatedAt     string           `json:"createdAt"`
	Pack          PackManifest     `json:"pack"`
	Files         []PackBundleFile `json:"files,omitempty"`
}
