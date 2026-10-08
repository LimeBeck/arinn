// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

struct Command {
    name: &'static str,
    arguments: &'static str,
    minimum: usize,
    maximum: usize,
    description: &'static str,
}
macro_rules! command {
    ($name:literal, $arguments:literal, $min:literal, $max:literal, $description:literal) => {
        Command {
            name: $name,
            arguments: $arguments,
            minimum: $min,
            maximum: $max,
            description: $description,
        }
    };
}
// Help and validation share the supported command paths and argument counts.
const COMMANDS: &[Command] = &[
    command!("status", "", 0, 0, "Show daemon status (default command)"),
    command!("providers", "", 0, 0, "List providers and their state"),
    command!("capabilities", "", 0, 0, "List registered capabilities"),
    command!("activities", "", 0, 0, "List running application instances"),
    command!(
        "diagnostics",
        "",
        0,
        0,
        "Show errors and provider diagnostics"
    ),
    command!(
        "snapshot",
        "<provider>",
        1,
        1,
        "Show a provider's application snapshot"
    ),
    command!("provider restart", "<provider>", 1, 1, "Restart a provider"),
    command!("providers reload", "", 0, 0, "Reload provider manifests"),
    command!(
        "launch",
        "<provider> <application>",
        2,
        2,
        "Launch an application"
    ),
    command!(
        "activate",
        "<provider> <instance>",
        2,
        2,
        "Activate a running application"
    ),
    command!(
        "terminate",
        "<provider> <instance>",
        2,
        2,
        "Request a graceful application close"
    ),
    command!("home", "", 0, 0, "Show the Arinn Home overlay"),
    command!("back", "", 0, 0, "Return from Home to the saved activity"),
    command!("shortcuts", "", 0, 0, "List web shortcuts (default: list)"),
    command!("shortcuts list", "", 0, 0, "List saved web shortcuts"),
    command!(
        "shortcuts add",
        "<title> <url> [icon]",
        2,
        3,
        "Add a web shortcut"
    ),
    command!(
        "shortcuts edit",
        "<shortcut> <title> <url> [icon]",
        3,
        4,
        "Update a web shortcut"
    ),
    command!(
        "shortcuts remove",
        "<shortcut>",
        1,
        1,
        "Remove a web shortcut"
    ),
    command!(
        "shortcuts open",
        "<shortcut>",
        1,
        1,
        "Open a saved web shortcut"
    ),
    command!("browser tabs", "", 0, 0, "List browser windows and tabs"),
    command!(
        "browser activate",
        "<provider> <tab>",
        2,
        2,
        "Activate a browser tab"
    ),
    command!(
        "browser close",
        "<provider> <tab>",
        2,
        2,
        "Close a browser tab"
    ),
];

pub fn valid(args: &[String]) -> bool {
    COMMANDS.iter().any(|command| {
        let words: Vec<_> = command.name.split_whitespace().collect();
        args.len() >= words.len() + command.minimum
            && args.len() <= words.len() + command.maximum
            && words.iter().zip(args).all(|(word, arg)| word == arg)
    })
}

// Recognize help only in command position, never inside a positional value.
pub fn help(args: &[String]) -> Option<String> {
    let path = if args.first().is_some_and(|arg| arg == "help") {
        &args[1..]
    } else if args
        .last()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        &args[..args.len() - 1]
    } else {
        return None;
    };
    let path = path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    let commands: Vec<_> = COMMANDS
        .iter()
        .filter(|command| {
            path.is_empty() || command.name == path || command.name.starts_with(&format!("{path} "))
        })
        .collect();
    if commands.is_empty() {
        return None;
    }
    Some(render(&path, &commands))
}

pub fn usage_error(args: &[String]) -> String {
    let group = args.first().map(String::as_str).filter(|arg| {
        COMMANDS
            .iter()
            .any(|command| command.name.split_whitespace().next() == Some(arg))
    });
    let scope = group.map_or(String::new(), |group| format!(" {group}"));
    format!("Invalid usage. Try 'arinnctl{scope} --help'.")
}

fn render(path: &str, commands: &[&Command]) -> String {
    let leaf = commands.len() == 1 && commands[0].name == path;
    let mut out = if leaf {
        let command = commands[0];
        format!(
            "Usage: arinnctl [--json] {}{}{}\n\n{}\n",
            path,
            if command.arguments.is_empty() {
                ""
            } else {
                " "
            },
            command.arguments,
            command.description
        )
    } else {
        let suffix = if path.is_empty() {
            "".into()
        } else {
            format!(" {path}")
        };
        format!(
            "Usage: arinnctl [--json]{suffix} [COMMAND]\n\n{}\n",
            if path.is_empty() {
                "Control Arinn through its daemon. With no command, show status."
            } else {
                "Choose a subcommand below. Commands with no subcommand are listed explicitly."
            }
        )
    };
    out.push_str("\nOptions:\n  -h, --help     Show help without connecting to the daemon\n  -V, --version  Print the build id without connecting to the daemon\n  --json         Print read results as JSON; help remains plain text\n");
    if !leaf {
        out.push_str("\nCommands:\n");
        let mut rows = Vec::new();
        for command in commands {
            let relative = if path.is_empty() {
                command.name
            } else {
                command.name.strip_prefix(path).unwrap().trim_start()
            };
            let label = if path.is_empty() && relative.contains(' ') {
                continue;
            } else if path.is_empty() && ["providers", "shortcuts"].contains(&relative) {
                format!("{relative} [COMMAND]")
            } else if relative.is_empty() {
                "(no subcommand)".into()
            } else {
                format!(
                    "{relative}{}{}",
                    if command.arguments.is_empty() {
                        ""
                    } else {
                        " "
                    },
                    command.arguments
                )
            };
            rows.push((label, command.description));
        }
        if path.is_empty() {
            rows.push(("provider <COMMAND>".into(), "Manage provider lifecycle"));
            rows.push((
                "browser <COMMAND>".into(),
                "List, activate or close browser tabs",
            ));
        }
        let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
        for (label, description) in rows {
            out.push_str(&format!("  {label:width$}  {description}\n"));
        }
        out.push_str("\nUse 'arinnctl <command> --help' for arguments and subcommands.\n");
    }
    if commands.iter().any(|command| !command.arguments.is_empty()) {
        out.push_str("\nArguments and IDs:\n");
    }
    let argument_help = [
        ("provider", "Provider ID from 'arinnctl providers'"),
        (
            "application",
            "Application ID from 'arinnctl snapshot <provider>'",
        ),
        ("instance", "Running instance ID from a provider snapshot"),
        ("tab", "Tab ID from 'arinnctl browser tabs'"),
        ("shortcut", "Shortcut ID from 'arinnctl shortcuts list'"),
        ("title", "Shortcut title; quote it if it contains spaces"),
        ("url", "HTTPS URL, or HTTP URL for a local-network service"),
        ("icon", "Optional icon reference; omitted means empty"),
    ];
    for (argument, description) in argument_help {
        if path.is_empty()
            || commands
                .iter()
                .any(|command| command.arguments.contains(argument))
        {
            out.push_str(&format!("  {argument:11}  {description}\n"));
        }
    }
    out.push_str("\nExamples:\n  arinnctl providers\n  arinnctl --json status\n  arinnctl snapshot <provider>\n  arinnctl shortcuts add \"Example service\" https://example.org/\n");
    out.push_str("\nExit codes:\n  0  Success (including help)\n  1  Runtime or operation failure\n  2  Invalid usage\n  3  Daemon unavailable\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_accepts_its_argument_bounds_and_rejects_excess() {
        for command in COMMANDS {
            for count in command.minimum..=command.maximum {
                let mut args: Vec<String> =
                    command.name.split_whitespace().map(str::to_owned).collect();
                args.extend((0..count).map(|index| format!("synthetic-{index}")));
                assert!(valid(&args), "{args:?}");
            }
            let mut excess: Vec<String> =
                command.name.split_whitespace().map(str::to_owned).collect();
            excess.extend((0..=command.maximum).map(|index| format!("synthetic-{index}")));
            assert!(!valid(&excess), "{excess:?}");
            if command.minimum > 0 {
                let mut missing: Vec<String> =
                    command.name.split_whitespace().map(str::to_owned).collect();
                missing.extend((1..command.minimum).map(|index| format!("synthetic-{index}")));
                assert!(!valid(&missing), "{missing:?}");
            }
        }
    }
}
