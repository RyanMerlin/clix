package clix

func LoadPolicy(path string) (PolicyBundle, error) {
	var p PolicyBundle
	if err := readJSON(path, &p); err != nil {
		return PolicyBundle{}, err
	}
	return p, nil
}

func evalList(needle string, list []string) bool {
	if len(list) == 0 {
		return true
	}
	for _, v := range list {
		if v == "*" || v == needle {
			return true
		}
	}
	return false
}

func matchesRule(rule PolicyRule, ctx map[string]string, cap CapabilityManifest) bool {
	m := rule.Match
	if len(m.Capabilities) > 0 && !evalList(cap.Name, m.Capabilities) {
		return false
	}
	if len(m.Profiles) > 0 {
		profile := ctx["profile"]
		if !evalList(profile, m.Profiles) {
			return false
		}
	}
	if len(m.Envs) > 0 && !evalList(ctx["env"], m.Envs) {
		return false
	}
	if len(m.Risk) > 0 && !evalList(cap.Risk, m.Risk) {
		return false
	}
	if len(m.SideEffects) > 0 && !evalList(cap.SideEffectClass, m.SideEffects) {
		return false
	}
	if len(m.Backends) > 0 && !evalList(cap.Backend.Type, m.Backends) {
		return false
	}
	return true
}

func EvaluatePolicy(p PolicyBundle, ctx map[string]string, cap CapabilityManifest) map[string]any {
	for _, rule := range p.Rules {
		if matchesRule(rule, ctx, cap) {
			return map[string]any{
				"decision": rule.Effect,
				"reason":   fallback(rule.Reason, "Matched policy rule ("+rule.Effect+")"),
				"rule":     rule,
			}
		}
	}
	return map[string]any{
		"decision": p.DefaultDecision,
		"reason":   "No rule matched",
	}
}

func fallback(v, alt string) string {
	if v == "" {
		return alt
	}
	return v
}
