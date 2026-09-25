use std::fs;
use std::path::Path;
use std::process::Command;

use crate::support::repository_root;

const VERBS: [&str; 12] = [
    "check",
    "fix",
    "claim",
    "adopt",
    "render",
    "refs",
    "external-plan",
    "external-assess",
    "locale-assess",
    "locale-inventory",
    "policy-include",
    "record-set",
];

/// A shell value the documentation writes, spelled as the grammar would see
/// it once the shell ran: each flag's value by what the flag takes.
fn filled(flag: &str) -> &'static str {
    match flag {
        "--base" => "1111111111111111111111111111111111111111",
        "--candidate" => "2222222222222222222222222222222222222222",
        "--object-format" => "sha1",
        "--profile" => "observe",
        "--ref" | "--default-branch-ref" => "refs/heads/main",
        "--repository" => "github.com/acme/widget",
        "--forge" => "github",
        _ => "value",
    }
}

/// The commands a code block of one file writes: every line inside a fence,
/// or every line of a CI file, from the program name on, with its
/// backslash-continued lines joined. A placeholder such as `<path>` marks a
/// synopsis rather than a command, so such a line is left out.
fn commands(text: &str, every_line: bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut fenced = false;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !(fenced || every_line) {
            continue;
        }
        let Some(start) = ["./amiss-linux-x86_64 ", "amiss "]
            .iter()
            .find_map(|program| {
                line.match_indices(program)
                    .find(|(at, _)| {
                        let before = line.get(..*at).and_then(|head| head.chars().last());
                        let verb = line
                            .get(at.saturating_add(program.len())..)
                            .unwrap_or_default();
                        before.is_none_or(|character| {
                            !character.is_alphanumeric() && character != '-'
                        }) && VERBS.iter().any(|name| {
                            verb.strip_prefix(name)
                                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
                        })
                    })
                    .map(|(at, _)| at)
            })
        else {
            continue;
        };
        let mut command = line.get(start..).unwrap_or_default().to_owned();
        while command.ends_with('\\') {
            command.pop();
            let Some(next) = lines.next() else {
                break;
            };
            command.push(' ');
            command.push_str(next.trim());
        }
        let placeholder = command.match_indices('<').any(|(at, _)| {
            command
                .get(at.saturating_add(1)..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|character| character.is_ascii_lowercase())
        });
        if !placeholder {
            found.push(command);
        }
    }
    found
}

/// Splits one command as a shell would into words, up to the first pipe,
/// redirection or separator, and fills each shell expansion by its flag.
/// `--repo` always names a directory that does not exist, so no documented
/// command reads or writes this checkout while it is parsed.
fn argv(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    for character in command.chars() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, ' ' | '\t') => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            (Some(_) | None, _) => word.push(character),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut filled_words: Vec<String> = Vec::new();
    for word in words.into_iter().skip(1) {
        if ["|", "||", "&&", ";"].contains(&word.as_str()) || word.starts_with('>') {
            break;
        }
        let flag = filled_words.last().cloned().unwrap_or_default();
        if flag == "--repo" {
            filled_words.push("/nonexistent/amiss-documented-command".to_owned());
        } else if word == "$@" {
            // the GitLab template sets its positional arguments to the candidate
            filled_words.push("--candidate".to_owned());
            filled_words.push(filled("--candidate").to_owned());
        } else if word == "${identity[@]}" {
            for identity in ["--repository", "--forge", "--ref", "--default-branch-ref"] {
                filled_words.push(identity.to_owned());
                filled_words.push(filled(identity).to_owned());
            }
        } else if word.contains('$') {
            filled_words.push(filled(&flag).to_owned());
        } else {
            filled_words.push(word);
        }
    }
    filled_words
}

/// Every command the book, the README, the agent skill, the agentic recipe
/// and the GitLab template show runs through the real grammar and is accepted
/// by it, whatever happens after. A refused spelling, a symbolic ref where a
/// full id belongs, a flag the verb does not own, is documentation drift.
#[test]
fn every_documented_command_parses() {
    let root = repository_root();
    let mut sources: Vec<(String, bool)> = fs::read_dir(root.join("docs/src"))
        .expect("the book source is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .map(|path| (path.display().to_string(), false))
        .collect();
    sources.sort_unstable();
    for (file, every_line) in [
        ("README.md", false),
        ("docs/src/llms.txt", false),
        ("integrations/claude/skills/amiss/SKILL.md", false),
        ("integrations/gh-aw/docs-drift-fix.md", false),
        ("integrations/gitlab/amiss.gitlab-ci.yml", true),
    ] {
        sources.push((root.join(file).display().to_string(), every_line));
    }
    let mut checked = 0_usize;
    let mut refused = Vec::new();
    for (file, every_line) in sources {
        let text = fs::read_to_string(Path::new(&file)).expect("a documented source is readable");
        for command in commands(&text, every_line) {
            let words = argv(&command);
            let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
                .args(&words)
                .output()
                .expect("the binary runs");
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            );
            checked = checked.saturating_add(1);
            if [
                "INVALID_INVOCATION",
                "INVALID_EVENT",
                "INVALID_PROFILE",
                "invalid invocation",
            ]
            .iter()
            .any(|refusal| said.contains(refusal))
            {
                refused.push(format!(
                    "{file}: {command}\n  {}",
                    said.lines().nth(1).unwrap_or_default()
                ));
            }
        }
    }
    assert!(
        checked >= 15,
        "the extraction still finds the documented commands: {checked}"
    );
    assert!(
        refused.is_empty(),
        "documented commands the grammar refuses:\n{}",
        refused.join("\n")
    );
}
