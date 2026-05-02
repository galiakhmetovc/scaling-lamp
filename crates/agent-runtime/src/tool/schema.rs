use super::ToolDefinition;
use serde_json::{Value, json};

impl ToolDefinition {
    pub fn openai_function_schema(&self) -> Value {
        json!({
            "type": "function",
            "name": self.name.as_str(),
            "description": self.description,
            "parameters": self.name.input_schema(),
        })
    }
}
