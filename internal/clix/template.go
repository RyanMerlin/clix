package clix

import (
	"fmt"
	"regexp"
)

var tmplRe = regexp.MustCompile(`\$\{([^}]+)\}`)

func RenderTemplate(v any, scope map[string]any) any {
	switch val := v.(type) {
	case string:
		return tmplRe.ReplaceAllStringFunc(val, func(m string) string {
			match := tmplRe.FindStringSubmatch(m)
			if len(match) != 2 {
				return ""
			}
			resolved := lookup(scope, match[1])
			if resolved == nil {
				return ""
			}
			return fmt.Sprint(resolved)
		})
	case []any:
		out := make([]any, len(val))
		for i, item := range val {
			out[i] = RenderTemplate(item, scope)
		}
		return out
	case map[string]any:
		out := map[string]any{}
		for k, item := range val {
			out[k] = RenderTemplate(item, scope)
		}
		return out
	default:
		return v
	}
}

func lookup(scope map[string]any, expr string) any {
	cur := any(scope)
	for _, part := range splitDot(expr) {
		m, ok := cur.(map[string]any)
		if !ok {
			return nil
		}
		cur = m[part]
	}
	return cur
}

func splitDot(expr string) []string {
	var parts []string
	start := 0
	for i := 0; i < len(expr); i++ {
		if expr[i] == '.' {
			parts = append(parts, expr[start:i])
			start = i + 1
		}
	}
	parts = append(parts, expr[start:])
	return parts
}
