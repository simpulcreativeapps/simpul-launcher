use std::path::PathBuf;

use crate::models::{AgentOrigin, BuildProfile};
use crate::registry::{
    detect_integrations, import_into_registry, install_in_registry, load_or_init_registry,
    reconcile_installed_apps, resolve_launchable_target, save_registry, try_launch_app,
    uninstall_in_registry, ImportRequest,
};

pub enum CliDisposition {
    RunGui { screenshot_path: Option<PathBuf> },
    ExitCode(i32),
}

pub fn parse_and_dispatch(args: &[String]) -> CliDisposition {
    if args.is_empty() {
        return CliDisposition::RunGui {
            screenshot_path: None,
        };
    }

    // Check `--screenshot <file.png>` anywhere in arguments first
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--screenshot" {
            if let Some(p) = args.get(i + 1) {
                return CliDisposition::RunGui {
                    screenshot_path: Some(PathBuf::from(p)),
                };
            } else {
                eprintln!("error: --screenshot requires a file path (<file.png>)");
                return CliDisposition::ExitCode(2);
            }
        } else if let Some(rest) = args[i].strip_prefix("--screenshot=") {
            return CliDisposition::RunGui {
                screenshot_path: Some(PathBuf::from(rest)),
            };
        }
        i += 1;
    }

    match args[0].as_str() {
        "help" | "--help" | "-h" => {
            print_help();
            CliDisposition::ExitCode(0)
        }
        "--version" | "-V" | "version" => {
            println!("simpul-creative-launcher 1.0.0");
            CliDisposition::ExitCode(0)
        }
        "status" => CliDisposition::ExitCode(cmd_status(&args[1..])),
        "list" | "ls" => CliDisposition::ExitCode(cmd_list(&args[1..])),
        "import" => CliDisposition::ExitCode(cmd_import(&args[1..])),
        "install" => CliDisposition::ExitCode(cmd_install(&args[1..])),
        "uninstall" | "remove" => CliDisposition::ExitCode(cmd_uninstall(&args[1..])),
        "launch" | "run" | "open" => CliDisposition::ExitCode(cmd_launch(&args[1..])),
        "rescan" | "scan" => CliDisposition::ExitCode(cmd_rescan(&args[1..])),
        "agent-config" | "bridge" => CliDisposition::ExitCode(cmd_agent_config(&args[1..])),
        "--gui" => CliDisposition::RunGui {
            screenshot_path: None,
        },
        unknown if unknown.starts_with('-') => {
            // Unknown desktop flags shouldn't crash normal launcher open
            CliDisposition::RunGui {
                screenshot_path: None,
            }
        }
        unknown => {
            eprintln!("Unknown command '{unknown}'. Run with `--help` to see CLI commands.");
            CliDisposition::ExitCode(2)
        }
    }
}

fn cmd_status(args: &[String]) -> i32 {
    let as_json = args.iter().any(|a| a == "--json");
    let reg = load_or_init_registry(false);
    let tools = detect_integrations(&reg.settings);
    let deploy_ready = reg
        .apps
        .iter()
        .filter(|a| a.profile == BuildProfile::Release)
        .count();
    let installed = reg
        .apps
        .iter()
        .filter(|a| a.install_state.is_installed())
        .count();

    if as_json {
        let out = serde_json::json!({
            "launcher": "Simpul Creative Launcher",
            "version": crate::suite::LAUNCHER_VERSION,
            "deploy_gate_release_only": reg.settings.require_release_profile,
            "total_apps": reg.apps.len(),
            "deploy_ready_apps": deploy_ready,
            "installed_apps": installed,
            "registry_path": tools.registry_path,
            "install_root": reg.settings.install_root,
            "integrations": {
                "antigravity_agy": {
                    "detected": tools.agy_detected,
                    "path": tools.agy_path,
                },
                "claude": {
                    "detected": tools.claude_detected,
                    "path": tools.claude_path,
                },
                "cli_binary": tools.cli_self_path,
            }
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!(
            "Simpul Creative Launcher v{} — {} ({})",
            crate::suite::LAUNCHER_VERSION,
            crate::suite::MAKER_NAME,
            crate::suite::MAKER_URL
        );
        println!("  Registry       : {}", tools.registry_path);
        println!("  Install Root   : {}", reg.settings.install_root);
        println!(
            "  Deploy Gate    : {}",
            if reg.settings.require_release_profile {
                "Strict Release Only (Debug builds blocked)"
            } else {
                "Permissive"
            }
        );
        println!(
            "  Apps           : {} total, {} deploy-ready, {} installed",
            reg.apps.len(),
            deploy_ready,
            installed
        );
        println!(
            "  Antigravity    : {}",
            tools.agy_path.as_deref().unwrap_or("agy (awaiting PATH)")
        );
        println!(
            "  Claude CLI     : {}",
            tools
                .claude_path
                .as_deref()
                .unwrap_or("claude (awaiting PATH)")
        );
    }
    0
}

fn cmd_list(args: &[String]) -> i32 {
    let as_json = args.iter().any(|a| a == "--json");
    let deploy_only = args.iter().any(|a| a == "--deploy-only");
    let reg = load_or_init_registry(false);
    let filtered: Vec<_> = reg
        .apps
        .iter()
        .filter(|a| !deploy_only || a.profile.is_deploy_ready())
        .collect();

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&filtered).unwrap_or_default()
        );
    } else {
        println!(
            "{:<16} {:<16} {:<14} {:<18} {:<12} STATUS",
            "ID", "NAME", "PROFILE", "ORIGIN", "LATEST"
        );
        println!("{}", "-".repeat(92));
        for app in filtered {
            let st = match &app.install_state {
                crate::models::InstallState::Installed { version, .. } => {
                    if app.has_update() {
                        format!("Installed v{version} (Update -> v{})", app.latest_version)
                    } else {
                        format!("Installed v{version}")
                    }
                }
                crate::models::InstallState::NotInstalled => "Available".to_string(),
            };
            println!(
                "{:<16} {:<16} {:<14} {:<18} {:<12} {}",
                app.id,
                app.name,
                app.profile.short_label(),
                app.origin.label(),
                app.latest_version,
                st
            );
        }
    }
    0
}

fn cmd_import(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!(
            "Usage: simpul-creative-launcher import <path-or-manifest.json> [--name <name>] [--id <id>] [--version <v>] [--tagline <text>] [--category <cat>] [--agent claude|agy|cli] [--install] [--allow-debug] [--json]"
        );
        return 2;
    }

    let mut source_path: Option<PathBuf> = None;
    let mut id = None;
    let mut name = None;
    let mut version = None;
    let mut tagline = None;
    let mut category = None;
    let mut agent = AgentOrigin::CliManual;
    let mut allow_debug = false;
    let mut auto_install = false;
    let mut as_json = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--id" => {
                id = args.get(i + 1).cloned();
                i += 2;
            }
            "--name" => {
                name = args.get(i + 1).cloned();
                i += 2;
            }
            "--version" => {
                version = args.get(i + 1).cloned();
                i += 2;
            }
            "--tagline" => {
                tagline = args.get(i + 1).cloned();
                i += 2;
            }
            "--category" => {
                category = args.get(i + 1).cloned();
                i += 2;
            }
            "--agent" => {
                if let Some(v) = args.get(i + 1) {
                    agent = AgentOrigin::from_arg(v);
                }
                i += 2;
            }
            "--allow-debug" => {
                allow_debug = true;
                i += 1;
            }
            "--install" => {
                auto_install = true;
                i += 1;
            }
            "--json" => {
                as_json = true;
                i += 1;
            }
            val if !val.starts_with('-') && source_path.is_none() => {
                source_path = Some(PathBuf::from(val));
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let Some(path) = source_path else {
        eprintln!("error: missing <path-or-manifest.json> argument");
        return 2;
    };

    let mut reg = load_or_init_registry(false);
    match import_into_registry(
        &mut reg,
        ImportRequest {
            source_path: path,
            id,
            name,
            version,
            tagline,
            category,
            agent,
            allow_debug,
            auto_install,
        },
    ) {
        Ok(app) => {
            if let Err(e) = save_registry(&reg) {
                eprintln!("error: {e}");
                return 1;
            }
            if as_json {
                println!("{}", serde_json::to_string_pretty(&app).unwrap_or_default());
            } else {
                println!(
                    "Imported '{}' ({}) v{} via {} [{}]{}",
                    app.name,
                    app.id,
                    app.latest_version,
                    app.origin.label(),
                    app.profile.short_label(),
                    if app.install_state.is_installed() {
                        " and installed into Simpul Creative Launcher."
                    } else {
                        "."
                    }
                );
            }
            0
        }
        Err(e) => {
            if as_json {
                println!("{}", serde_json::json!({ "ok": false, "error": e }));
            } else {
                eprintln!("error: {e}");
            }
            1
        }
    }
}

fn cmd_install(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!(
            "Usage: simpul-creative-launcher install <app-id-or-path> [--agent claude|agy|cli] [--allow-debug] [--json]"
        );
        return 2;
    }

    let mut target: Option<String> = None;
    let mut agent = AgentOrigin::CliManual;
    let mut allow_debug = false;
    let mut as_json = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent" => {
                if let Some(v) = args.get(i + 1) {
                    agent = AgentOrigin::from_arg(v);
                }
                i += 2;
            }
            "--allow-debug" => {
                allow_debug = true;
                i += 1;
            }
            "--json" => {
                as_json = true;
                i += 1;
            }
            val if !val.starts_with('-') && target.is_none() => {
                target = Some(val.to_string());
                i += 1;
            }
            _ => i += 1,
        }
    }

    let Some(target_str) = target else {
        eprintln!("error: missing <app-id-or-path>");
        return 2;
    };

    let mut reg = load_or_init_registry(false);
    let candidate_path = PathBuf::from(&target_str);

    // If Claude/agy passes a file/folder path directly to `install`, import + install in one step!
    let result = if candidate_path.exists()
        && !reg
            .apps
            .iter()
            .any(|a| a.id.eq_ignore_ascii_case(&target_str))
    {
        import_into_registry(
            &mut reg,
            ImportRequest {
                source_path: candidate_path,
                id: None,
                name: None,
                version: None,
                tagline: None,
                category: None,
                agent,
                allow_debug,
                auto_install: true,
            },
        )
    } else {
        install_in_registry(&mut reg, &target_str, agent, allow_debug)
    };

    match result {
        Ok(app) => {
            if let Err(e) = save_registry(&reg) {
                eprintln!("error: {e}");
                return 1;
            }
            if as_json {
                println!("{}", serde_json::to_string_pretty(&app).unwrap_or_default());
            } else {
                println!(
                    "Installed '{}' v{} [{}] via {}.",
                    app.name,
                    app.latest_version,
                    app.profile.short_label(),
                    agent.label()
                );
            }
            0
        }
        Err(e) => {
            if as_json {
                println!("{}", serde_json::json!({ "ok": false, "error": e }));
            } else {
                eprintln!("error: {e}");
            }
            1
        }
    }
}

fn cmd_uninstall(args: &[String]) -> i32 {
    let Some(app_id) = args.iter().find(|a| !a.starts_with('-')) else {
        eprintln!("Usage: simpul-creative-launcher uninstall <app-id>");
        return 2;
    };
    let as_json = args.iter().any(|a| a == "--json");
    let mut reg = load_or_init_registry(false);
    match uninstall_in_registry(&mut reg, app_id, AgentOrigin::CliManual) {
        Ok(name) => {
            let _ = save_registry(&reg);
            if as_json {
                println!(
                    "{}",
                    serde_json::json!({ "ok": true, "removed": name, "id": app_id })
                );
            } else {
                println!("Uninstalled '{name}'.");
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cmd_launch(args: &[String]) -> i32 {
    let Some(app_id) = args.first() else {
        eprintln!("Usage: simpul-creative-launcher launch <app-id> [-- <args>...]");
        return 2;
    };
    let extra: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|a| a.as_str() != "--")
        .cloned()
        .collect();
    let reg = load_or_init_registry(false);
    let Some(app) = reg
        .apps
        .iter()
        .find(|a| a.id.eq_ignore_ascii_case(app_id) || a.name.eq_ignore_ascii_case(app_id))
    else {
        eprintln!("error: app '{app_id}' not found");
        return 1;
    };
    // Status merah yang sudah pernah diterima tetap berlaku di CLI (dibaca dari cache, tanpa jaringan).
    {
        let mid = crate::suite::manifest_id_for(&app.id);
        let version = app
            .install_state
            .installed_version()
            .unwrap_or(&app.latest_version)
            .to_string();
        let mut guard = simpul_guard::Guard::new(mid, &version, crate::suite::guard_cache_dir());
        guard.sources.clear();
        if let simpul_guard::Verdict::Disabled(notice) = guard.check().verdict {
            eprintln!("error: {} dihentikan: {}", app.name, notice.title);
            return 1;
        }
    }
    match try_launch_app(app, &extra) {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cmd_rescan(args: &[String]) -> i32 {
    let as_json = args.iter().any(|a| a == "--json");
    let mut reg = load_or_init_registry(false);
    let updated = reconcile_installed_apps(&mut reg);
    let _ = save_registry(&reg);
    let linked: Vec<_> = reg
        .apps
        .iter()
        .filter_map(|a| {
            resolve_launchable_target(a).map(|p| {
                serde_json::json!({
                    "id": a.id,
                    "name": a.name,
                    "version": a.install_state.installed_version().unwrap_or(&a.latest_version),
                    "executable": p.display().to_string(),
                    "launch_flags": a.launch_flags,
                })
            })
        })
        .collect();
    if as_json {
        let out = serde_json::json!({
            "updated": updated,
            "linked_apps": linked,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!("Simpul PC Rescan complete ({} launchable apps linked):", linked.len());
        for app in &linked {
            println!(
                "  - {} ({}) -> {}",
                app["name"].as_str().unwrap_or(""),
                app["version"].as_str().unwrap_or(""),
                app["executable"].as_str().unwrap_or("")
            );
        }
    }
    0
}

fn cmd_agent_config(_args: &[String]) -> i32 {
    let reg = load_or_init_registry(false);
    let tools = detect_integrations(&reg.settings);
    println!(
        "# Simpul Creative Launcher — Claude & Antigravity (agy) Bridge\n\
         \n\
         Binary: `{bin}`\n\
         Registry: `{reg_path}`\n\
         \n\
         ## 1. Import & Install a Deploy-Ready App (Claude / agy)\n\
         ```bash\n\
         \"{bin}\" import ./simpul-app.json --agent claude --install --json\n\
         \"{bin}\" install ./target/release/my-simpul-app.exe --agent agy --json\n\
         ```\n\
         \n\
         ## 2. Inspect Deploy-Ready Catalog (JSON)\n\
         ```bash\n\
         \"{bin}\" list --deploy-only --json\n\
         \"{bin}\" status --json\n\
         ```\n\
         \n\
         ## 3. `simpul-app.json` Manifest Format\n\
         ```json\n\
         {{\n\
           \"id\": \"simpul-new-tool\",\n\
           \"name\": \"Simpul New Tool\",\n\
           \"version\": \"1.0.0\",\n\
           \"profile\": \"release\",\n\
           \"tagline\": \"Production creative utility\",\n\
           \"category\": \"Simpul Suite\",\n\
           \"binary\": \"./target/release/simpul-new-tool.exe\"\n\
         }}\n\
         ```",
        bin = tools.cli_self_path,
        reg_path = tools.registry_path,
    );
    0
}

pub fn print_help() {
    println!(
        "Simpul Creative Launcher 1.0.0\n\
         Desktop launcher for deploy-ready Simpul software integrated with CLI, Antigravity (agy), and Claude.\n\
         \n\
         USAGE:\n\
           simpul-creative-launcher                         Open desktop launcher window\n\
           simpul-creative-launcher --screenshot <file.png> Render 120 frames unfocused & save PNG\n\
           simpul-creative-launcher status [--json]         Show launcher & Claude/agy integration status\n\
           simpul-creative-launcher list [--deploy-only] [--json]\n\
           simpul-creative-launcher import <path> [--name <n>] [--version <v>] [--agent claude|agy|cli] [--install]\n\
           simpul-creative-launcher install <id-or-path> [--agent claude|agy|cli]\n\
           simpul-creative-launcher uninstall <id>\n\
           simpul-creative-launcher launch <id> [-- <args>...]\n\
           simpul-creative-launcher rescan [--json]         Detect installed Simpul apps on this PC\n\
           simpul-creative-launcher agent-config            Print Claude & Antigravity (agy) integration guide"
    );
}
