use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

// ── Default prompts ──────────────────────────────────────────────────────

/// Default compact prompt, adapted from Claude Code's compaction behavior.
const DEFAULTS: &[(&str, &str)] = &[(
    "summary",
    r##"
CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.\n\n- Do NOT use Read, Bash, Grep, Glob, Edit, Write, or ANY other tool.\n- You already have all the context you need in the conversation above.\n- Tool calls will be REJECTED and will waste your only turn — you will fail the task.\n- Your entire response must be plain text: an <analysis> block followed by a <summary> block.\n\nYour task is to create a detailed summary of the conversation so far, paying close attention to the user's explicit requests and your previous actions.\nThis summary should be thorough in capturing technical details, code patterns, and architectural decisions that would be essential for continuing development work without losing context.\n\nBefore providing your final summary, wrap your analysis in <analysis> tags to organize your thoughts and ensure you've covered all necessary points. In your analysis process:\n\n1. Chronologically analyze each message and section of the conversation. For each section thoroughly identify:\n   - The user's explicit requests and intents\n   - Your approach to addressing the user's requests\n   - Key decisions, technical concepts and code patterns\n   - Specific details like:\n     - file names\n     - full code snippets\n     - function signatures\n     - file edits\n   - Errors that you ran into and how you fixed them\n   - Pay special attention to specific user feedback that you received, especially if the user told you to do something differently.\n   - Note any security-relevant instructions or constraints the user stated (e.g., sensitive files or data to avoid, operations that must not be performed, credential or secret handling rules). These MUST be preserved verbatim in the summary so they continue to apply after compaction.\n2. Double-check for technical accuracy and completeness, addressing each required element thoroughly.\n\nYour summary should include the following sections:\n\n1. Primary Request and Intent: Capture all of the user's explicit requests and intents in detail\n2. Key Technical Concepts: List all important technical concepts, technologies, and frameworks discussed.\n3. Files and Code Sections: Enumerate specific files and code sections examined, modified, or created. Pay special attention to the most recent messages and include full code snippets where applicable and include a summary of why this file read or edit is important.\n4. Errors and fixes: List all errors that you ran into, and how you fixed them. Pay special attention to specific user feedback that you received, especially if the user told you to do something differently.\n5. Problem Solving: Document problems solved and any ongoing troubleshooting efforts.\n6. All user messages: List ALL user messages that are not tool results. These are critical for understanding the users' feedback and changing intent. Preserve any security-relevant instructions or constraints verbatim so they remain in effect after compaction.\n7. Pending Tasks: Outline any pending tasks that you have explicitly been asked to work on.\n8. Current Work: Describe in detail precisely what was being worked on immediately before this summary request, paying special attention to the most recent messages from both user and assistant. Include file names and code snippets where applicable.\n9. Optional Next Step: List the next step that you will take that is related to the most recent work you were doing. IMPORTANT: ensure that this step is DIRECTLY in line with the user's most recent explicit requests, and the task you were working on immediately before this summary request. If your last task was concluded, then only list next steps if they are explicitly in line with the users request. Do not start on tangential requests or really old requests that were already completed without confirming with the user first.\n                       If there is a next step, include direct quotes from the most recent conversation showing exactly what task you were working on and where you left off. This should be verbatim to ensure there's no drift in task interpretation.\n\nHere's an example of how your output should be structured:\n\n<example>\n<analysis>\n[Your thought process, ensuring all points are covered thoroughly and accurately]\n</analysis>\n\n<summary>\n1. Primary Request and Intent:\n   [Detailed description]\n\n2. Key Technical Concepts:\n   - [Concept 1]\n   - [Concept 2]\n   - [...]\n\n3. Files and Code Sections:\n   - [File Name 1]\n      - [Summary of why this file is important]\n      - [Summary of the changes made to this file, if any]\n      - [Important Code Snippet]\n   - [File Name 2]\n      - [Important Code Snippet]\n   - [...]\n\n4. Errors and fixes:\n    - [Detailed description of error 1]:\n      - [How you fixed the error]\n      - [User feedback on the error if any]\n    - [...]\n\n5. Problem Solving:\n   [Description of solved problems and ongoing troubleshooting]\n\n6. All user messages: \n    - [Detailed non tool use user message]\n    - [...]\n\n7. Pending Tasks:\n   - [Task 1]\n   - [Task 2]\n   - [...]\n\n8. Current Work:\n   [Precise description of current work]\n\n9. Optional Next Step:\n   [Optional Next step to take]\n\n</summary>\n</example>\n\nPlease provide your summary based on the conversation so far, following this structure and ensuring precision and thoroughness in your response. \n\nThere may be additional summarization instructions provided in the included context. If so, remember to follow these instructions when creating the above summary. Examples of instructions include:\n<example>\n## Compact Instructions\nWhen summarizing the conversation focus on typescript code changes and also remember the mistakes you made and how you fixed them.\n</example>\n\n<example>\n# Summary instructions\nWhen you are using compact - please focus on test output and code changes. Include file reads verbatim.\n</example>\n\n\nREMINDER: Do NOT call any tools. Respond with plain text only — an <analysis> block followed by a <summary> block. Tool calls will be rejected and you will fail the task.
"##,
)];

// ── Prompt store ─────────────────────────────────────────────────────────

/// Thread-safe prompt store that loads `.md` files from disk, with built-in defaults.
pub struct Prompt {
    prompts: Arc<RwLock<HashMap<String, String>>>,
}

impl Clone for Prompt {
    fn clone(&self) -> Self {
        Self {
            prompts: Arc::clone(&self.prompts),
        }
    }
}

impl Prompt {
    /// Load all `.md` files from `dir`, falling back to built-in defaults.
    pub fn load_from_dir(dir: PathBuf) -> Self {
        let mut prompts: HashMap<String, String> = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    match std::fs::read_to_string(&path) {
                        Ok(c) => {
                            prompts.insert(stem.into(), c.trim().into());
                        }
                        Err(e) => {
                            tracing::warn!(key=%stem, error=%e, "Failed to read prompt file");
                        }
                    }
                }
            }
        }
        for &(key, def) in DEFAULTS {
            prompts.entry(key.into()).or_insert_with(|| def.into());
        }

        Self {
            prompts: Arc::new(RwLock::new(prompts)),
        }
    }

    pub fn set(&self, key: &str, value: String) {
        if let Ok(mut g) = self.prompts.write() {
            g.insert(key.into(), value.trim().into());
        }
    }

    /// Look up a prompt by key; falls back to defaults, then empty string.
    pub fn get(&self, key: &str) -> String {
        if let Ok(g) = self.prompts.read()
            && let Some(v) = g.get(key)
        {
            return v.clone();
        }
        DEFAULTS
            .iter()
            .find(|&&(k, _)| k == key)
            .map(|&(_, v)| v)
            .unwrap_or("")
            .into()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    fn d() -> PathBuf {
        std::env::temp_dir().join(format!("pt_{}", std::process::id()))
    }
    #[test]
    fn test_unknown() {
        assert_eq!(Prompt::load_from_dir(d()).get("x"), "");
    }
    #[test]
    fn test_set_get() {
        let p = Prompt::load_from_dir(d());
        p.set("k", "v".into());
        assert_eq!(p.get("k"), "v");
    }
    #[test]
    fn test_default() {
        assert!(
            Prompt::load_from_dir(d())
                .get("summary")
                .contains("CRITICAL")
        );
    }
    #[test]
    fn test_load() {
        let dir = std::env::temp_dir().join(format!("pt2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("w.md"), "W").unwrap();
        let p = Prompt::load_from_dir(dir.clone());
        assert_eq!(p.get("w"), "W");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn test_thread_safe() {
        let p = Arc::new(Prompt::load_from_dir(d()));
        let (a, b) = (p.clone(), p.clone());
        std::thread::spawn(move || a.set("a", "1".into()))
            .join()
            .unwrap();
        std::thread::spawn(move || b.set("b", "2".into()))
            .join()
            .unwrap();
        assert_eq!(p.get("a"), "1");
        assert_eq!(p.get("b"), "2");
    }
}
