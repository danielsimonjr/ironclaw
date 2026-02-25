//! Shell completion generation.

use clap::CommandFactory;

/// Generate shell completion scripts.
pub fn generate_completions(shell: &str) -> anyhow::Result<()> {
    let mut cmd = crate::cli::Cli::command();
    let bin_name = cmd.get_name().to_string();

    match shell.to_lowercase().as_str() {
        "bash" => {
            clap_complete::generate(
                clap_complete::Shell::Bash,
                &mut cmd,
                &bin_name,
                &mut std::io::stdout(),
            );
        }
        "zsh" => {
            clap_complete::generate(
                clap_complete::Shell::Zsh,
                &mut cmd,
                &bin_name,
                &mut std::io::stdout(),
            );
        }
        "fish" => {
            clap_complete::generate(
                clap_complete::Shell::Fish,
                &mut cmd,
                &bin_name,
                &mut std::io::stdout(),
            );
        }
        "powershell" | "ps" => {
            clap_complete::generate(
                clap_complete::Shell::PowerShell,
                &mut cmd,
                &bin_name,
                &mut std::io::stdout(),
            );
        }
        "elvish" => {
            clap_complete::generate(
                clap_complete::Shell::Elvish,
                &mut cmd,
                &bin_name,
                &mut std::io::stdout(),
            );
        }
        _ => {
            anyhow::bail!(
                "Unsupported shell: {}. Supported: bash, zsh, fish, powershell, elvish",
                shell
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_shell_returns_error() {
        let result = generate_completions("nushell");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unsupported shell"));
        assert!(msg.contains("nushell"));
    }

    #[test]
    fn known_shells_are_recognized() {
        for shell in &["bash", "zsh", "fish", "powershell", "ps", "elvish"] {
            let result = generate_completions(shell);
            assert!(result.is_ok(), "shell '{}' should be supported", shell);
        }
    }

    #[test]
    fn case_insensitive_shell_names() {
        for shell in &["BASH", "Zsh", "FISH", "PowerShell", "PS", "ELVISH"] {
            let result = generate_completions(shell);
            assert!(
                result.is_ok(),
                "shell '{}' should be recognized (case-insensitive)",
                shell
            );
        }
    }
}
