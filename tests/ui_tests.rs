use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tui_crudo::events::{Actor, DocumentStage};
use tui_crudo::state::{AppState, ConversationMessage, DocumentProgressState};
use tui_crudo::ui::UI;

#[test]
fn test_terminal_resolutions_rendering() {
    let sizes = [(80, 24), (100, 30), (120, 40), (160, 50), (40, 10), (30, 5)];

    for (w, h) in sizes {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut ui = UI::new("assets/crudo.png");
        let mut state = AppState::new();
        state.terminal_size = (w, h);

        // Add a message
        state
            .conversation
            .add_message(ConversationMessage::new_user(
                "Explain P&IDs.".to_string(),
                Vec::new(),
            ));
        state
            .conversation
            .add_message(ConversationMessage::new_crudo(
                "A P&ID is a Piping and Instrumentation Diagram...".to_string(),
            ));

        // Must draw successfully without panicking
        terminal
            .draw(|f| {
                ui.render(f, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.area.width, w);
        assert_eq!(buffer.area.height, h);
    }
}

#[test]
fn test_empty_input_cursor_position_in_ui() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let state = AppState::new();

    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();

    // With margin (1) + app frame border (1) + input box border (1), inner area begins at x = 3
    let cursor = terminal.get_cursor_position().unwrap();
    assert_eq!(
        cursor.x, 3,
        "Cursor X must be at column 3 (start of input box inner area within frame and margin)"
    );
    assert_ne!(
        cursor.x,
        3 + "Type a message...  /help for commands".len() as u16,
        "Cursor must NOT be at placeholder length"
    );
}

#[test]
fn test_chat_scrolling_and_auto_scroll_behavior() {
    let mut state = AppState::new();

    // Add multiple messages to make conversation tall
    for i in 1..=10 {
        state
            .conversation
            .add_message(ConversationMessage::new_user(
                format!("Message {i}"),
                Vec::new(),
            ));
        state
            .conversation
            .add_message(ConversationMessage::new_crudo(format!("Response {i}")));
    }

    assert!(state.conversation.auto_scroll);
    let initial_bottom = state.conversation.scroll_offset;
    assert_eq!(initial_bottom, state.conversation.max_scroll());

    // User scrolls upward to inspect older messages
    state.conversation.scroll_up(6);
    assert!(!state.conversation.auto_scroll);
    assert_eq!(
        state.conversation.scroll_offset,
        initial_bottom.saturating_sub(6)
    );

    // New message arrives while user is scrolled up
    state
        .conversation
        .add_message(ConversationMessage::new_crudo(
            "New incoming response while scrolled up".to_string(),
        ));

    // Must NOT force back to bottom; position must remain stable
    assert!(!state.conversation.auto_scroll);
    assert_eq!(
        state.conversation.scroll_offset,
        initial_bottom.saturating_sub(6)
    );

    // User scrolls to bottom
    state.conversation.scroll_to_bottom();
    assert!(state.conversation.auto_scroll);
    assert_eq!(
        state.conversation.scroll_offset,
        state.conversation.max_scroll()
    );
}

#[test]
fn test_scroll_keyboard_controls() {
    let mut app = tui_crudo::App::new(None, "assets/crudoo.png");

    for i in 1..=15 {
        app.state
            .conversation
            .add_message(ConversationMessage::new_user(
                format!("User {i}"),
                Vec::new(),
            ));
        app.state
            .conversation
            .add_message(ConversationMessage::new_crudo(format!("Crudo {i}")));
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let initial_max = app.state.conversation.max_scroll();

    // When input is empty, KeyCode::Up scrolls conversation up by 1 line
    rt.block_on(async {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
    });
    assert!(!app.state.conversation.auto_scroll);
    assert_eq!(
        app.state.conversation.scroll_offset,
        initial_max.saturating_sub(1)
    );

    // KeyCode::PageUp scrolls conversation by a page
    rt.block_on(async {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
    });
    assert!(app.state.conversation.scroll_offset < initial_max.saturating_sub(1));

    // KeyCode::Home scrolls to top (0)
    rt.block_on(async {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Home,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
    });
    assert!(!app.state.conversation.auto_scroll);
    assert_eq!(app.state.conversation.scroll_offset, 0);

    // KeyCode::Down scrolls down by 1 line
    rt.block_on(async {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
    });
    assert_eq!(app.state.conversation.scroll_offset, 1);

    // KeyCode::End scrolls to bottom
    rt.block_on(async {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::End,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
    });
    assert!(app.state.conversation.auto_scroll);
    assert_eq!(
        app.state.conversation.scroll_offset,
        app.state.conversation.max_scroll()
    );
}

#[test]
fn test_header_metrics_and_status_bar_separation() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();
    state.system.cpu_usage = Some(18.0);
    state.system.gpu_usage = Some(34);

    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let text = buffer
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();

    // Header must contain TIME, DATE, CPU, GPU
    assert!(text.contains("TIME:"), "Header must display TIME");
    assert!(text.contains("DATE:"), "Header must display DATE");
    assert!(text.contains("CPU:"), "Header must display CPU");
    assert!(text.contains("GPU:"), "Header must display GPU");
    assert!(text.contains("18%"), "Header must display actual CPU value");
    assert!(text.contains("34%"), "Header must display actual GPU value");

    // Status bar must contain MODEL, MCP, SANDBOX, NET
    assert!(text.contains("MODEL:"), "Status bar must display MODEL");
    assert!(text.contains("MCP:"), "Status bar must display MCP");
    assert!(text.contains("SANDBOX:"), "Status bar must display SANDBOX");
    assert!(text.contains("NET:"), "Status bar must display NET");
}

#[test]
fn test_document_progress_event_rendering() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();

    let mut doc_state =
        DocumentProgressState::new("doc_1".to_string(), "inspection_report.pdf".to_string());
    doc_state.stage = DocumentStage::Parsing;
    doc_state.current = 18;
    doc_state.total = 25;
    doc_state.percentage = 72;
    state.conversation.active_document = Some(doc_state);

    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let buffer_str: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(buffer_str.contains("Reading inspection_report.pdf"));
    assert!(buffer_str.contains("Parsing document"));
    assert!(buffer_str.contains("72%"));
    assert!(buffer_str.contains("18 / 25"));
}

#[test]
fn test_truthful_disconnected_backend_submission() {
    let mut app = tui_crudo::App::new(None, "assets/crudoo.png");
    assert_eq!(app.state.conversation.messages.len(), 0);

    // Simulate user typing a prompt
    app.state.input.insert_str("What is the state of plant A?");
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
    });

    // Conversation should now have exactly 2 messages:
    // 1. User message
    // 2. Truthful CRUDO message stating backend is not connected (NO fake response!)
    assert_eq!(app.state.conversation.messages.len(), 2);
    assert_eq!(app.state.conversation.messages[0].role, Actor::User);
    assert_eq!(
        app.state.conversation.messages[0].content,
        "What is the state of plant A?"
    );
    assert_eq!(app.state.conversation.messages[1].role, Actor::Crudo);
    assert!(app.state.conversation.messages[1]
        .content
        .contains("not connected"));
}

#[test]
fn test_status_bar_truthful_values() {
    let state = AppState::new();
    assert_eq!(
        state.backend.connection,
        tui_crudo::events::BackendConnectionStatus::NotConnected
    );
    assert_eq!(
        state.backend.model,
        tui_crudo::events::ModelStatus::NotConnected
    );
    assert_eq!(
        state.backend.mcp,
        tui_crudo::events::McpStatus::NotConnected
    );
    assert_eq!(
        state.backend.sandbox,
        tui_crudo::events::SandboxStatus::NotConnected
    );
}

#[test]
fn test_wrapped_messages_and_content_height() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();

    // Very long user prompt that wraps across multiple lines
    let long_user_msg = "Analyze this extremely long inspection report containing many pages of engineering data, piping specs, instrumentation tags, safety relief valves, and heat exchangers across multiple offshore production platforms.";
    state
        .conversation
        .add_message(ConversationMessage::new_user(
            long_user_msg.to_string(),
            Vec::new(),
        ));

    // Very long CRUDO response that wraps across multiple lines
    let long_crudo_msg = "Piping and Instrumentation Diagrams (P&IDs) show the piping of the process flow together with the installed equipment and instrumentation. They contain line numbers, pipe sizes, valves, controllers, transmitters, and other process details essential for offshore operations.";
    state
        .conversation
        .add_message(ConversationMessage::new_crudo(long_crudo_msg.to_string()));

    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();

    let total_rows = state.conversation.last_total_rows.get();
    // Verify that wrapped content accounts for far more rows than just 2 messages
    assert!(
        total_rows >= 12,
        "Total rendered rows ({total_rows}) should account for wrapped text, borders, and spacing"
    );
}

#[test]
fn test_fixed_input_location_during_scrolling() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();

    for i in 1..=20 {
        state
            .conversation
            .add_message(ConversationMessage::new_user(
                format!("Prompt {i}"),
                Vec::new(),
            ));
        state
            .conversation
            .add_message(ConversationMessage::new_crudo(format!("Answer {i}")));
    }

    // Draw at bottom
    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();
    let cursor_at_bottom = terminal.get_cursor_position().unwrap();

    // Scroll upward
    state.conversation.scroll_up(10);
    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();
    let cursor_while_scrolled = terminal.get_cursor_position().unwrap();

    // The user input position must remain strictly fixed while conversation scrolls!
    assert_eq!(
        cursor_at_bottom, cursor_while_scrolled,
        "User input cursor position must remain strictly fixed regardless of chat scroll offset"
    );
}

#[test]
fn test_slash_help_command_integration() {
    let mut app = tui_crudo::App::new(None, "assets/crudoo.png");
    app.state.input.insert_str("/help");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
    });

    assert_eq!(app.state.conversation.messages.len(), 1);
    assert_eq!(app.state.conversation.messages[0].role, Actor::Crudo);
    assert!(app.state.conversation.messages[0]
        .content
        .contains("SHORTCUTS"));
    assert!(app.state.conversation.messages[0].content.contains("—"));
    assert!(app.state.conversation.messages[0].content.contains("/help"));
    assert!(app.state.conversation.messages[0]
        .content
        .contains("/clear"));
    assert!(app.state.conversation.messages[0]
        .content
        .contains("/status"));
}

#[test]
fn test_visual_layout_output() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();
    state.system.cpu_usage = Some(18.0);
    state.system.gpu_usage = Some(34);
    state
        .conversation
        .add_message(ConversationMessage::new_user(
            "Explain P&IDs.".to_string(),
            Vec::new(),
        ));
    state
        .conversation
        .add_message(ConversationMessage::new_crudo(
            "A P&ID is a Piping and Instrumentation Diagram showing processes.".to_string(),
        ));

    terminal.draw(|f| ui.render(f, &state)).unwrap();
    let buf = terminal.backend().buffer();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        println!("{line}");
    }
}

#[test]
fn test_scroll_indicator_rendering() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();

    for i in 1..=5 {
        state
            .conversation
            .add_message(ConversationMessage::new_user(
                format!("Prompt {i}"),
                Vec::new(),
            ));
        state
            .conversation
            .add_message(ConversationMessage::new_crudo(format!("Response {i}")));
    }

    // Initial draw to establish dimensions
    terminal.draw(|f| ui.render(f, &state)).unwrap();

    // Scroll up
    state.conversation.scroll_up(4);

    terminal.draw(|f| ui.render(f, &state)).unwrap();
    let buf = terminal.backend().buffer();
    let text = buf.content().iter().map(|c| c.symbol()).collect::<String>();
    assert!(
        text.contains("lines above bottom"),
        "Scroll indicator should appear when scrolled up"
    );
}

#[test]
fn test_status_labels_styling() {
    use ratatui::style::{Color, Modifier};

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();
    state.system.cpu_usage = Some(12.0);
    state.system.gpu_usage = Some(34);

    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let label_color = Color::Rgb(0x83, 0x68, 0xFE); // #8368FE

    // Check cells for label words
    let mut found_time = false;
    let mut found_model = false;

    for y in 0..buffer.area.height {
        let mut row_text = String::new();
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            row_text.push_str(cell.symbol());
            if cell.symbol() == "T"
                && cell.fg == label_color
                && cell.modifier.contains(Modifier::BOLD)
            {
                found_time = true;
            }
            if cell.symbol() == "M"
                && cell.fg == label_color
                && cell.modifier.contains(Modifier::BOLD)
            {
                found_model = true;
            }
        }
    }

    assert!(found_time, "TIME label must be colored #8368FE and BOLD");
    assert!(found_model, "MODEL label must be colored #8368FE and BOLD");
}

#[test]
fn test_deep_scrolling_and_history_retention() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();

    // Insert 25 messages of varying sizes with blank lines
    for i in 1..=25 {
        state
            .conversation
            .add_message(ConversationMessage::new_user(
                format!("User message {i}\nwith multiple lines\n\nand blank spaces"),
                Vec::new(),
            ));
        state.conversation.add_message(ConversationMessage::new_crudo(
            format!("CRUDO response {i}: Detailed diagnostic response line 1\nDetailed diagnostic response line 2"),
        ));
    }

    assert_eq!(state.conversation.messages.len(), 50);

    // Render initial view at bottom
    terminal.draw(|f| ui.render(f, &state)).unwrap();
    assert!(state.conversation.auto_scroll);
    let bottom_scroll = state.conversation.scroll_offset;
    assert!(
        bottom_scroll > 50,
        "Content height should be substantially greater than viewport"
    );

    // Scroll all the way to top using HOME
    state.conversation.scroll_to_top();
    assert_eq!(state.conversation.scroll_offset, 0);
    assert!(!state.conversation.auto_scroll);

    terminal.draw(|f| ui.render(f, &state)).unwrap();
    let buf_top = terminal.backend().buffer();
    let top_text = buf_top
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(
        top_text.contains("User message 1"),
        "First message must be visible at top scroll"
    );

    // Scroll down 1 line with DOWN
    state.conversation.scroll_down(1);
    assert_eq!(state.conversation.scroll_offset, 1);

    // Scroll up 1 line with UP
    state.conversation.scroll_up(1);
    assert_eq!(state.conversation.scroll_offset, 0);

    // Scroll down a page
    state.conversation.scroll_page_down(10);
    assert_eq!(state.conversation.scroll_offset, 10);

    // Scroll up a page
    state.conversation.scroll_page_up(10);
    assert_eq!(state.conversation.scroll_offset, 0);

    // Add a new message while user is reading at top
    state
        .conversation
        .add_message(ConversationMessage::new_crudo(
            "Incoming asynchronous notification".to_string(),
        ));
    assert_eq!(state.conversation.messages.len(), 51);
    assert!(
        !state.conversation.auto_scroll,
        "Auto-scroll must NOT trigger when reading older messages"
    );
    assert_eq!(
        state.conversation.scroll_offset, 0,
        "Scroll position must not jump when new message arrives"
    );

    // Scroll to bottom using END
    state.conversation.scroll_to_bottom();
    assert!(
        state.conversation.auto_scroll,
        "Auto-scroll must be restored at bottom"
    );
    terminal.draw(|f| ui.render(f, &state)).unwrap();
    let buf_bottom = terminal.backend().buffer();
    let bottom_text = buf_bottom
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(
        bottom_text.contains("Incoming asynchronous notification"),
        "Newest message must be visible at bottom"
    );
}

#[test]
fn test_section_9_resizing_and_all_criteria() {
    let mut app = tui_crudo::App::new(None, "assets/crudoo.png");

    // Test with:
    // - many short messages
    // - long wrapped CRUDO responses
    // - long USER messages
    // - alternating USER/CRUDO messages
    // - messages containing blank lines
    for i in 1..=10 {
        let user_content = if i % 2 == 0 {
            format!("Short msg {i}")
        } else {
            format!(
                "Long user message {i} with extensive detail about piping, instrumentation, diagrams, control valves, pressure transducers, and offshore platforms.\n\nExtra paragraph with whitespace."
            )
        };
        app.state
            .conversation
            .add_message(ConversationMessage::new_user(user_content, Vec::new()));

        let crudo_content = if i % 2 == 0 {
            format!("Short response {i}")
        } else {
            format!(
                "Long CRUDO analysis response {i} detailing system telemetry, process stream characteristics, mass balances, safety instrumented systems (SIS), and emergency shutdown valves (ESD).\nLine 2 of analysis.\n\nLine 4 after blank line."
            )
        };
        app.state
            .conversation
            .add_message(ConversationMessage::new_crudo(crudo_content));
    }

    // Criteria 13: All 20 conversation messages remain intact in state
    assert_eq!(app.state.conversation.messages.len(), 20);

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Test multiple resolutions (Criteria 15)
    for (width, height) in [(80, 24), (100, 30), (120, 40), (160, 50)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();

        // Simulate resize event
        app.state.terminal_size = (width, height);
        terminal.draw(|f| app.ui.render(f, &app.state)).unwrap();

        let initial_max = app.state.conversation.max_scroll();
        assert!(
            initial_max > 0,
            "Max scroll must be positive for long conversation"
        );

        // Criteria 8: New messages auto-scroll when at bottom
        assert!(app.state.conversation.auto_scroll);

        // Criteria 2 & 14: UP reveals older messages
        rt.block_on(async {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Up,
                crossterm::event::KeyModifiers::empty(),
            ))
            .await;
        });
        assert!(!app.state.conversation.auto_scroll);
        assert_eq!(
            app.state.conversation.scroll_offset,
            initial_max.saturating_sub(1)
        );

        // Criteria 4: PageUp works
        let page = app.state.conversation.last_viewport_height.get().max(4);
        rt.block_on(async {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::PageUp,
                crossterm::event::KeyModifiers::empty(),
            ))
            .await;
        });
        assert_eq!(
            app.state.conversation.scroll_offset,
            initial_max.saturating_sub(1 + page)
        );

        // Criteria 6: HOME reaches the beginning
        rt.block_on(async {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Home,
                crossterm::event::KeyModifiers::empty(),
            ))
            .await;
        });
        assert_eq!(app.state.conversation.scroll_offset, 0);

        terminal.draw(|f| app.ui.render(f, &app.state)).unwrap();
        let buf = terminal.backend().buffer();
        let top_str = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        // Criteria 1: Oldest message is accessible
        assert!(top_str.contains("Long user message 1"));

        // Criteria 9: New message does NOT auto-scroll while reading older messages
        app.state
            .conversation
            .add_message(ConversationMessage::new_crudo(
                "Another async update".to_string(),
            ));
        assert!(!app.state.conversation.auto_scroll);
        assert_eq!(app.state.conversation.scroll_offset, 0);

        // Criteria 3: DOWN moves toward newer messages
        rt.block_on(async {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Down,
                crossterm::event::KeyModifiers::empty(),
            ))
            .await;
        });
        assert_eq!(app.state.conversation.scroll_offset, 1);

        // Criteria 5: PageDown moves toward newer messages
        rt.block_on(async {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::PageDown,
                crossterm::event::KeyModifiers::empty(),
            ))
            .await;
        });
        assert_eq!(app.state.conversation.scroll_offset, 1 + page);

        // Criteria 7: END reaches the latest message
        rt.block_on(async {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::End,
                crossterm::event::KeyModifiers::empty(),
            ))
            .await;
        });
        assert!(app.state.conversation.auto_scroll);
        assert_eq!(
            app.state.conversation.scroll_offset,
            app.state.conversation.max_scroll()
        );

        terminal.draw(|f| app.ui.render(f, &app.state)).unwrap();
        let buf_end = terminal.backend().buffer();
        let end_str = buf_end
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(end_str.contains("Another async update"));

        // Criteria 10, 11, 12: Fixed layout elements
        // Status bar is at the bottom row (height - margin - 2)
        assert!(end_str.contains("MODEL:"));
        assert!(end_str.contains("MCP:"));
        assert!(end_str.contains("SANDBOX:"));
        assert!(end_str.contains("NET:"));
    }
}

#[test]
fn test_touchpad_two_finger_scrolling() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};

    let mut app = tui_crudo::App::new(None, "assets/crudoo.png");

    for i in 1..=20 {
        app.state
            .conversation
            .add_message(ConversationMessage::new_user(
                format!("Touchpad query {i}"),
                Vec::new(),
            ));
        app.state
            .conversation
            .add_message(ConversationMessage::new_crudo(format!(
                "Touchpad answer {i}"
            )));
    }

    let initial_max = app.state.conversation.max_scroll();
    assert!(initial_max > 10);
    assert!(app.state.conversation.auto_scroll);
    assert_eq!(app.state.conversation.scroll_offset, initial_max);

    // 1. Two-finger swipe UP: MouseEventKind::ScrollUp
    // -> chat moves toward older messages by 1 line
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 20,
        row: 10,
        modifiers: KeyModifiers::empty(),
    });

    assert!(!app.state.conversation.auto_scroll);
    assert_eq!(
        app.state.conversation.scroll_offset,
        initial_max.saturating_sub(1)
    );

    // 2. Additional ScrollUp
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 20,
        row: 10,
        modifiers: KeyModifiers::empty(),
    });
    assert_eq!(
        app.state.conversation.scroll_offset,
        initial_max.saturating_sub(2)
    );

    // 3. Receive new message while scrolled up -> viewport must NOT jump to bottom
    let current_offset = app.state.conversation.scroll_offset;
    app.state
        .conversation
        .add_message(ConversationMessage::new_crudo(
            "Background incoming event".to_string(),
        ));
    assert!(!app.state.conversation.auto_scroll);
    assert_eq!(app.state.conversation.scroll_offset, current_offset);

    // 4. Two-finger swipe DOWN: MouseEventKind::ScrollDown
    // -> chat moves toward newer messages by 1 line
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 20,
        row: 10,
        modifiers: KeyModifiers::empty(),
    });
    assert_eq!(app.state.conversation.scroll_offset, current_offset + 1);

    // 5. Scroll all the way down with ScrollDown
    for _ in 0..100 {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 20,
            row: 10,
            modifiers: KeyModifiers::empty(),
        });
    }
    assert!(app.state.conversation.auto_scroll);
    assert_eq!(
        app.state.conversation.scroll_offset,
        app.state.conversation.max_scroll()
    );

    // 6. Scroll past top (clamp check: 0 <= scroll_offset <= max_scroll)
    for _ in 0..200 {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 20,
            row: 10,
            modifiers: KeyModifiers::empty(),
        });
    }
    assert_eq!(app.state.conversation.scroll_offset, 0);
    assert!(!app.state.conversation.auto_scroll);
}

#[test]
fn test_user_message_box_integrated_border_and_color() {
    use ratatui::style::Color;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudoo.png");
    let mut state = AppState::new();

    let user_prompt = "Explain steam turbines in thermal power stations.";
    state
        .conversation
        .add_message(ConversationMessage::new_user(
            user_prompt.to_string(),
            Vec::new(),
        ));

    terminal
        .draw(|f| {
            ui.render(f, &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let expected_purple = Color::Rgb(193, 173, 249); // #C1ADF9
    let expected_white = Color::Rgb(0xFF, 0xFF, 0xFF);

    let mut found_user_top_border = false;
    let mut found_user_purple_triangle = false;
    let mut found_user_purple_label = false;
    let mut found_white_user_text = false;

    for y in 0..buffer.area.height {
        let mut row_symbols = String::new();
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            row_symbols.push_str(cell.symbol());

            // Check triangle styling
            if cell.symbol() == "▶" {
                assert_eq!(cell.fg, expected_purple, "Triangle must use hex #C1ADF9");
                found_user_purple_triangle = true;
            }

            // Check white text for message content
            if cell.symbol() == "s" && cell.fg == expected_white {
                found_white_user_text = true;
            }
        }
        println!("{row_symbols}");

        // Check that top border line contains ▶ USER and ─
        if row_symbols.contains("▶ USER") && row_symbols.contains('─') {
            found_user_top_border = true;
            // Verify the USER characters on this border row have color #C1ADF9
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol() == "U"
                    || cell.symbol() == "S"
                    || cell.symbol() == "E"
                    || cell.symbol() == "R"
                {
                    assert_eq!(
                        cell.fg, expected_purple,
                        "USER label on the border must use hex #C1ADF9"
                    );
                    found_user_purple_label = true;
                }
                if cell.symbol() == "─" {
                    assert_eq!(
                        cell.fg, expected_purple,
                        "Top border dashes must use hex #C1ADF9"
                    );
                }
            }
        }
    }

    assert!(
        found_user_top_border,
        "User message top border must contain '▶ USER ──'"
    );
    assert!(
        found_user_purple_triangle,
        "Triangle must be present in #C1ADF9"
    );
    assert!(
        found_user_purple_label,
        "USER label must be present in #C1ADF9"
    );
    assert!(found_white_user_text, "User prompt text must be white");
}

#[test]
fn test_event_batching_and_smooth_scrolling() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};

    let mut app = tui_crudo::App::new(None, "assets/crudoo.png");

    for i in 1..=30 {
        app.state
            .conversation
            .add_message(ConversationMessage::new_user(
                format!("Query {i}"),
                Vec::new(),
            ));
        app.state
            .conversation
            .add_message(ConversationMessage::new_crudo(format!("Response {i}")));
    }

    let initial_max = app.state.conversation.max_scroll();
    assert!(initial_max > 20);

    // Simulate high-frequency burst of 10 ScrollUp events from a touchpad swipe
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for _ in 0..10 {
            app.process_event(tui_crudo::event::AppEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 10,
                row: 10,
                modifiers: KeyModifiers::empty(),
            }))
            .await;
        }
    });

    // 10 events should scroll exactly 10 lines smoothly
    assert_eq!(
        app.state.conversation.scroll_offset,
        initial_max.saturating_sub(10)
    );
    assert!(!app.state.conversation.auto_scroll);

    // Simulate 5 ScrollDown events
    rt.block_on(async {
        for _ in 0..5 {
            app.process_event(tui_crudo::event::AppEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 10,
                row: 10,
                modifiers: KeyModifiers::empty(),
            }))
            .await;
        }
    });

    assert_eq!(
        app.state.conversation.scroll_offset,
        initial_max.saturating_sub(5)
    );
}

#[test]
fn test_exact_crudo_logo_and_user_message_visual() {
    use ratatui::style::Color;

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut ui = UI::new("assets/crudo.png");
    let mut state = AppState::new();

    state
        .conversation
        .add_message(ConversationMessage::new_user(
            "Checking new visual styles.".to_string(),
            Vec::new(),
        ));

    terminal.draw(|f| ui.render(f, &state)).unwrap();
    let buffer = terminal.backend().buffer();

    let logo_purple = Color::Rgb(170, 70, 255);
    let user_purple = Color::Rgb(193, 173, 249); // #C1ADF9
    let text_white = Color::Rgb(255, 255, 255);

    // 1 & 2: Verify CRUDO logo appears in top-left with purple RGB(170, 70, 255)
    // Logo line 0 has ▄▄█ ▄ starting at x=6, y=2 (inner area within margin=1, frame=1)
    let mut found_logo_char = false;
    for y in 2..10 {
        for x in 2..80 {
            let cell = &buffer[(x, y)];
            if (cell.symbol() == "▄" || cell.symbol() == "█" || cell.symbol() == "▀")
                && cell.fg == logo_purple
            {
                found_logo_char = true;
                break;
            }
        }
    }
    assert!(
        found_logo_char,
        "CRUDO logo characters must be rendered in top-left with RGB(170, 70, 255)"
    );

    // 3, 4, 5, 6, 7: Verify USER triangle, label, top, left, right, bottom borders are all #C1ADF9
    let mut found_triangle = false;
    let mut found_user_label = false;
    let mut found_top_border = false;
    let mut found_left_border = false;
    let mut found_right_border = false;
    let mut found_bottom_border = false;
    let mut found_white_text = false;

    for y in 0..buffer.area.height {
        let mut row_symbols = String::new();
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            row_symbols.push_str(cell.symbol());

            // Check triangle
            if cell.symbol() == "▶" && cell.fg == user_purple {
                found_triangle = true;
            }

            // Check white message text
            if cell.symbol() == "C" && cell.fg == text_white {
                found_white_text = true;
            }
        }

        // Check top border
        if row_symbols.contains("▶ USER") && row_symbols.contains('─') {
            found_top_border = true;
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol() == "U"
                    || cell.symbol() == "S"
                    || cell.symbol() == "E"
                    || cell.symbol() == "R"
                {
                    assert_eq!(cell.fg, user_purple, "USER label must be #C1ADF9");
                    found_user_label = true;
                }
                if cell.symbol() == "─" || cell.symbol() == "┐" {
                    assert_eq!(cell.fg, user_purple, "Top border must be #C1ADF9");
                }
            }
        }

        // Check left and right borders of user message content row (inner box, x in 3..width - 3)
        if row_symbols.contains("Checking new visual styles.") {
            for x in 3..buffer.area.width - 3 {
                let cell = &buffer[(x, y)];
                if cell.symbol() == "│" {
                    assert_eq!(
                        cell.fg, user_purple,
                        "User box side borders must be #C1ADF9"
                    );
                    if !found_left_border {
                        found_left_border = true;
                    } else {
                        found_right_border = true;
                    }
                }
            }
        }

        // Check user box bottom border (starts with two spaces indent: "  └")
        if row_symbols.contains("  └") && row_symbols.contains('┘') {
            found_bottom_border = true;
            for x in 4..buffer.area.width - 4 {
                let cell = &buffer[(x, y)];
                if cell.symbol() == "└" || cell.symbol() == "─" || cell.symbol() == "┘" {
                    assert_eq!(
                        cell.fg, user_purple,
                        "User box bottom border must be #C1ADF9"
                    );
                }
            }
        }
    }

    assert!(found_triangle, "USER triangle ▶ must be #C1ADF9");
    assert!(found_user_label, "USER label must be #C1ADF9");
    assert!(
        found_top_border,
        "USER top border must be integrated and #C1ADF9"
    );
    assert!(found_left_border, "USER left border must be #C1ADF9");
    assert!(found_right_border, "USER right border must be #C1ADF9");
    assert!(found_bottom_border, "USER bottom border must be #C1ADF9");
    assert!(found_white_text, "USER message text must be WHITE");
}
