use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Represents a single section in a meeting template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSection {
    /// Section title in English (e.g., "Summary", "Action Items")
    pub title: String,

    /// Instruction for the LLM on what to extract/include
    pub instruction: String,

    /// Format type: "paragraph", "list", or "string"
    pub format: String,

    /// Optional markdown formatting hint for list items (e.g., table structure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_format: Option<String>,

    /// Alternative formatting hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_item_format: Option<String>,

    /// Optional translations for the section title, keyed by ISO 639-1 language code
    /// (e.g., {"fr": "Résumé", "de": "Zusammenfassung"})
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_translations: Option<HashMap<String, String>>,
}

/// Represents a complete meeting template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Template display name
    pub name: String,

    /// Brief description of the template's purpose
    pub description: String,

    /// List of sections in the template
    pub sections: Vec<TemplateSection>,
}

impl TemplateSection {
    /// Returns the section title in the requested language, falling back to English.
    pub fn resolve_title<'a>(&'a self, language: &str) -> &'a str {
        self.title_translations
            .as_ref()
            .and_then(|t| t.get(language))
            .map(String::as_str)
            .unwrap_or(&self.title)
    }
}

impl Template {
    /// Validates the template structure
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Template name cannot be empty".to_string());
        }

        if self.description.is_empty() {
            return Err("Template description cannot be empty".to_string());
        }

        if self.sections.is_empty() {
            return Err("Template must have at least one section".to_string());
        }

        for (i, section) in self.sections.iter().enumerate() {
            if section.title.is_empty() {
                return Err(format!("Section {} has empty title", i));
            }

            if section.instruction.is_empty() {
                return Err(format!("Section '{}' has empty instruction", section.title));
            }

            match section.format.as_str() {
                "paragraph" | "list" | "string" => {},
                other => return Err(format!(
                    "Section '{}' has invalid format '{}'. Must be 'paragraph', 'list', or 'string'",
                    section.title, other
                )),
            }
        }

        Ok(())
    }

    /// Generates a clean markdown template structure with content placeholders.
    ///
    /// Uses H2 section headers (translated if `language` is provided) and
    /// format-specific placeholders so the LLM clearly understands where and
    /// what to write in each section.
    pub fn to_markdown_structure(&self, language: &str) -> String {
        let mut markdown = String::from("# [Meeting Title]\n\n");

        for section in &self.sections {
            // Use the translated title when available, otherwise fall back to English
            let title = section.resolve_title(language);
            // H2 headers make section boundaries unambiguous to the model
            markdown.push_str(&format!("## {}\n\n", title));

            let item_fmt = section.item_format.as_ref()
                .or(section.example_item_format.as_ref());

            match section.format.as_str() {
                "list" => {
                    if let Some(fmt) = item_fmt {
                        let fmt_str = fmt.as_str();
                        if fmt_str.trim_start().starts_with('|') {
                            // Markdown table — show the header + separator + a placeholder row
                            // so the model understands it must add real data rows
                            markdown.push_str(fmt_str);
                            markdown.push_str("\n| [value] | ... |\n\n");
                        } else {
                            // Non-table item format (e.g. "- [Name] ([Role])")
                            // Repeat 3 times so the model understands the repetition pattern
                            for _ in 0..3 {
                                markdown.push_str(fmt_str);
                                markdown.push('\n');
                            }
                            markdown.push('\n');
                        }
                    } else {
                        markdown.push_str("- [Item 1]\n- [Item 2]\n- [Item 3]\n\n");
                    }
                }
                "paragraph" => {
                    markdown.push_str("[Write a detailed paragraph based on the transcript]\n\n");
                }
                "string" => {
                    // Single-line metadata value (date, name, short text)
                    markdown.push_str("[Short metadata value based on the transcript]\n\n");
                }
                _ => {
                    markdown.push_str("[Content based on the transcript]\n\n");
                }
            }
        }

        markdown
    }

    /// Generates section-specific instructions for the LLM.
    ///
    /// Section titles in the instructions match the translated titles used in
    /// `to_markdown_structure()` so the model can correlate them.
    pub fn to_section_instructions(&self, language: &str) -> String {
        let mut instructions = String::from(
            "- **For the main title (`# [AI-Generated Title]`):** Analyze the entire transcript and create a concise, descriptive title for the meeting.\n"
        );

        for section in &self.sections {
            let title = section.resolve_title(language);
            instructions.push_str(&format!(
                "- **For the '{}' section:** {}.\n",
                title, section.instruction
            ));

            // Add item format instructions if present
            let item_format = section.item_format.as_ref()
                .or(section.example_item_format.as_ref());

            if let Some(format) = item_format {
                instructions.push_str(&format!(
                    "  - Items in this section should follow the format: `{}`.\n",
                    format
                ));
            }
        }

        instructions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_template() {
        let template = Template {
            name: "Test Template".to_string(),
            description: "A test template".to_string(),
            sections: vec![
                TemplateSection {
                    title: "Summary".to_string(),
                    instruction: "Provide a summary".to_string(),
                    format: "paragraph".to_string(),
                    item_format: None,
                    example_item_format: None,
                    title_translations: None,
                },
            ],
        };

        assert!(template.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_name() {
        let template = Template {
            name: "".to_string(),
            description: "A test template".to_string(),
            sections: vec![],
        };

        assert!(template.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_format() {
        let template = Template {
            name: "Test".to_string(),
            description: "Test".to_string(),
            sections: vec![
                TemplateSection {
                    title: "Test".to_string(),
                    instruction: "Test".to_string(),
                    format: "invalid".to_string(),
                    item_format: None,
                    example_item_format: None,
                    title_translations: None,
                },
            ],
        };

        assert!(template.validate().is_err());
    }
}
