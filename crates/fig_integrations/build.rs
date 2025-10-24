const CODEX_FOLDER: &str = "src/shell/inline_shell_completion";

// The order here is very specific, do no edit without understanding the implications
const CODEX_FILES: &[&str] = &[
    "guard_start.zsh",
    "LICENSE",
    "config.zsh",
    "util.zsh",
    "bind.zsh",
    "highlight.zsh",
    "widgets.zsh",
    "strategies/inline.zsh",
    "strategies/completion.zsh",
    "strategies/history.zsh",
    "strategies/match_prev_cmd.zsh",
    "fetch.zsh",
    "async.zsh",
    "start.zsh",
    "guard_end.zsh",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = std::path::Path::new(&out_dir);

    let mut inline_shell_completion = String::new();
    for file in CODEX_FILES {
        let path = std::path::Path::new(CODEX_FOLDER).join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        inline_shell_completion.push_str(&std::fs::read_to_string(path).unwrap());
    }

    // Replace template variables with actual binary names
    const CLI_BINARY_NAME: &str = "kiro-cli";
    const CLI_BINARY_NAME_UNDERSCORE: &str = "kiro_cli";

    inline_shell_completion =
        inline_shell_completion.replace("{{CLI_BINARY_NAME_UNDERSCORE}}", CLI_BINARY_NAME_UNDERSCORE);
    inline_shell_completion = inline_shell_completion.replace("{{CLI_BINARY_NAME}}", CLI_BINARY_NAME);

    std::fs::write(out_dir.join("inline_shell_completion.zsh"), inline_shell_completion).unwrap();
}
