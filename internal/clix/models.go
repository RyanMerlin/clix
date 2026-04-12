package clix

type CapabilityBackend struct {
	Type         string   `json:"type"`
	Name         string   `json:"name,omitempty"`
	Command      string   `json:"command,omitempty"`
	Args         []string `json:"args,omitempty"`
	CwdFromInput string   `json:"cwdFromInput,omitempty"`
	URL          string   `json:"url,omitempty"`
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
