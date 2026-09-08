mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let socket =
        std::env::var("ORBIT_SOCKET_NAME").unwrap_or_else(|_| "orbit-workspace-daemon".to_string());
    let token = std::env::var("ORBIT_DAEMON_TOKEN").expect("ORBIT_DAEMON_TOKEN must be set");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(commands::ClientState(orbit_client::DaemonClient::new(
            socket, token,
        )))
        .invoke_handler(tauri::generate_handler![
            commands::list_workspaces,
            commands::get_governance_policy,
            commands::save_governance_policy,
            commands::get_workspace_governance,
            commands::save_workspace_governance,
            commands::list_governance_presets,
            commands::create_governance_preset,
            commands::delete_governance_preset,
            commands::list_governance_rules,
            commands::create_governance_rule,
            commands::update_governance_rule,
            commands::delete_governance_rule,
            commands::list_secrets,
            commands::create_secret,
            commands::replace_secret,
            commands::delete_secret,
            commands::list_secret_assignments,
            commands::assign_secret,
            commands::unassign_secret,
            commands::create_workspace,
            commands::list_models,
            commands::set_workspace_model,
            commands::list_apps,
            commands::set_workspace_apps,
            commands::create_routine,
            commands::list_routines,
            commands::set_bot_settings,
            commands::list_bot_settings,
            commands::update_routine,
            commands::delete_routine,
            commands::create_skill,
            commands::list_skills,
            commands::update_skill,
            commands::delete_skill,
            commands::save_template,
            commands::list_templates,
            commands::delete_template,
            commands::transition_workspace,
            commands::runtime_status,
            commands::reconcile_runtime,
            commands::start_workspace,
            commands::stop_workspace,
            commands::reset_workspace,
            commands::delete_workspace,
            commands::create_desktop_session,
            commands::start_agent_session,
            commands::send_agent_prompt,
            commands::poll_agent_events,
            commands::resolve_agent_approval,
            commands::interrupt_agent,
            commands::stop_agent_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
