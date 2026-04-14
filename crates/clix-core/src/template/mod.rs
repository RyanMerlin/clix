use crate::error::{ClixError, Result};

pub fn render_args(args: &[String], ctx: &serde_json::Value) -> Result<Vec<String>> {
    let mut env = minijinja::Environment::new();
    args.iter().enumerate().map(|(i, arg)| {
        if !arg.contains("{{") { return Ok(arg.clone()); }
        let name = format!("arg_{i}");
        env.add_template_owned(name.clone(), arg.clone())
            .map_err(|e| ClixError::TemplateRender(e.to_string()))?;
        let tpl = env.get_template(&name).map_err(|e| ClixError::TemplateRender(e.to_string()))?;
        tpl.render(ctx).map_err(|e| ClixError::TemplateRender(e.to_string()))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_substitution() {
        let args = vec!["get".to_string(), "pods".to_string(), "-n".to_string(), "{{ input.namespace }}".to_string()];
        let ctx = serde_json::json!({"input":{"namespace":"production"},"context":{"env":"prod"}});
        assert_eq!(render_args(&args, &ctx).unwrap(), vec!["get","pods","-n","production"]);
    }

    #[test]
    fn test_no_template_passthrough() {
        let args = vec!["get".to_string(), "nodes".to_string()];
        let ctx = serde_json::json!({"input":{},"context":{}});
        assert_eq!(render_args(&args, &ctx).unwrap(), vec!["get","nodes"]);
    }
}
