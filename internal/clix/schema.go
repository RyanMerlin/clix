package clix

import "fmt"

func typeOf(v any) string {
	switch v.(type) {
	case nil:
		return "null"
	case []any:
		return "array"
	case map[string]any:
		return "object"
	case string:
		return "string"
	case bool:
		return "boolean"
	case float64, float32, int, int64, int32, uint, uint64:
		return "number"
	default:
		return fmt.Sprintf("%T", v)
	}
}

func ValidateSchema(schema map[string]any, value any, path string) []string {
	if schema == nil {
		return nil
	}
	var errs []string
	if t, ok := schema["type"].(string); ok && t != "" {
		actual := typeOf(value)
		if actual != t {
			errs = append(errs, fmt.Sprintf("%s must be of type %s, got %s", emptyOr(path, "value"), t, actual))
			return errs
		}
	}
	if enum, ok := schema["enum"].([]any); ok {
		found := false
		for _, item := range enum {
			if item == value {
				found = true
				break
			}
		}
		if !found {
			errs = append(errs, fmt.Sprintf("%s must be one of %v", emptyOr(path, "value"), enum))
			return errs
		}
	}
	obj, _ := value.(map[string]any)
	if obj != nil {
		if req, ok := schema["required"].([]any); ok {
			for _, raw := range req {
				key, _ := raw.(string)
				if _, ok := obj[key]; !ok {
					errs = append(errs, fmt.Sprintf("%s.%s is required", emptyOr(path, ""), key))
				}
			}
		}
		if props, ok := schema["properties"].(map[string]any); ok {
			for key, child := range props {
				if childSchema, ok := child.(map[string]any); ok {
					if v, ok := obj[key]; ok {
						errs = append(errs, ValidateSchema(childSchema, v, joinPath(path, key))...)
					}
				}
			}
		}
		if ap, ok := schema["additionalProperties"].(bool); ok && !ap {
			allowed := map[string]struct{}{}
			if props, ok := schema["properties"].(map[string]any); ok {
				for key := range props {
					allowed[key] = struct{}{}
				}
			}
			for key := range obj {
				if _, ok := allowed[key]; !ok {
					errs = append(errs, fmt.Sprintf("%s.%s is not allowed", emptyOr(path, ""), key))
				}
			}
		}
	}
	arr, _ := value.([]any)
	if arr != nil {
		if items, ok := schema["items"].(map[string]any); ok {
			for i, item := range arr {
				errs = append(errs, ValidateSchema(items, item, fmt.Sprintf("%s[%d]", path, i))...)
			}
		}
	}
	return errs
}

func emptyOr(v, fallback string) string {
	if v == "" {
		return fallback
	}
	return v
}

func joinPath(prefix, child string) string {
	if prefix == "" {
		return child
	}
	return prefix + "." + child
}
