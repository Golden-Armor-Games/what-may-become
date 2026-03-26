use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeroOrigin {
    Leader,
    Defender,
    Wanderer,
    Survivor,
}

impl HeroOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            HeroOrigin::Leader => "leader",
            HeroOrigin::Defender => "defender",
            HeroOrigin::Wanderer => "wanderer",
            HeroOrigin::Survivor => "survivor",
        }
    }
}

#[derive(Resource, Default)]
pub struct OpeningEventState {
    pub seen_opening: bool,
    pub origin: Option<HeroOrigin>,
    pub fame_local: i32,
    phase: OpeningPhase,
    fade_timer: f32,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum OpeningPhase {
    #[default]
    NotStarted,
    FadingIn,
    ShowingIntro,
    ShowingChoices,
    FadingOut,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttackChoice {
    Lead,
    Defend,
    Run,
    Cower,
}

#[derive(Component)]
struct OpeningOverlay;

#[derive(Component)]
struct IntroText;

#[derive(Component)]
struct EventPanel;

#[derive(Component)]
struct ChoiceButton(AttackChoice);

#[derive(Event)]
pub struct TriggerOpeningEvent;

#[derive(Event)]
pub struct OpeningEventComplete {
    pub origin: HeroOrigin,
    pub fame_gained: i32,
}

pub struct OpeningEventPlugin;

impl Plugin for OpeningEventPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpeningEventState>()
            .add_event::<TriggerOpeningEvent>()
            .add_event::<OpeningEventComplete>()
            .add_systems(Update, (
                trigger_opening_event,
                update_opening_event,
                handle_choice_buttons,
            ));
    }
}

fn trigger_opening_event(
    mut events: EventReader<TriggerOpeningEvent>,
    mut state: ResMut<OpeningEventState>,
    mut commands: Commands,
) {
    for _ in events.read() {
        if state.seen_opening || state.phase != OpeningPhase::NotStarted {
            continue;
        }

        state.phase = OpeningPhase::FadingIn;
        state.fade_timer = 0.0;

        // Spawn the dark overlay
        commands.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            OpeningOverlay,
        ));
    }
}

fn update_opening_event(
    mut state: ResMut<OpeningEventState>,
    time: Res<Time>,
    mut overlay_q: Query<(Entity, &mut BackgroundColor), With<OpeningOverlay>>,
    intro_q: Query<Entity, With<IntroText>>,
    mut commands: Commands,
) {
    let Ok((overlay_entity, mut bg_color)) = overlay_q.get_single_mut() else { return };

    match state.phase {
        OpeningPhase::FadingIn => {
            state.fade_timer += time.delta_secs();
            let alpha = (state.fade_timer / 1.5).min(1.0);
            bg_color.0 = Color::srgba(0.0, 0.0, 0.0, alpha * 0.9);

            if state.fade_timer >= 1.5 {
                state.phase = OpeningPhase::ShowingIntro;
                state.fade_timer = 0.0;

                // Spawn intro text
                commands.entity(overlay_entity).with_children(|parent| {
                    parent.spawn((
                        Text::new("The year is uncertain. The realm of Aethermoor stirs."),
                        TextFont {
                            font_size: 24.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.9, 0.85, 0.7)),
                        IntroText,
                    ));
                });
            }
        }
        OpeningPhase::ShowingIntro => {
            state.fade_timer += time.delta_secs();

            if state.fade_timer >= 3.0 {
                state.phase = OpeningPhase::ShowingChoices;
                state.fade_timer = 0.0;

                // Remove intro text
                for entity in intro_q.iter() {
                    commands.entity(entity).despawn_recursive();
                }

                // Spawn the event panel
                spawn_event_panel(&mut commands, overlay_entity);
            }
        }
        OpeningPhase::ShowingChoices => {
            // Handled by button interaction
        }
        OpeningPhase::FadingOut => {
            state.fade_timer += time.delta_secs();
            let alpha = 1.0 - (state.fade_timer / 0.5).min(1.0);
            bg_color.0 = Color::srgba(0.0, 0.0, 0.0, alpha * 0.9);

            if state.fade_timer >= 0.5 {
                state.phase = OpeningPhase::Complete;
                state.seen_opening = true;

                // Despawn everything
                commands.entity(overlay_entity).despawn_recursive();
            }
        }
        _ => {}
    }
}

fn spawn_event_panel(commands: &mut Commands, overlay_entity: Entity) {
    commands.entity(overlay_entity).with_children(|parent| {
        // Main panel container
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(30.0)),
                border: UiRect::all(Val::Px(3.0)),
                ..default()
            },
            BorderColor(Color::srgb(0.6, 0.5, 0.3)),
            BackgroundColor(Color::srgba(0.1, 0.08, 0.05, 0.95)),
            EventPanel,
        )).with_children(|panel| {
            // Title
            panel.spawn((
                Text::new("YOUR TOWN IS UNDER ATTACK"),
                TextFont {
                    font_size: 28.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.3, 0.2)),
                Node {
                    margin: UiRect::bottom(Val::Px(20.0)),
                    ..default()
                },
            ));

            // Description
            panel.spawn((
                Text::new("An unknown force descends on your\nvillage at dawn. The screaming\nwakes you. You have moments to act.\n\nWhat do you do?"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.82, 0.75)),
                Node {
                    margin: UiRect::bottom(Val::Px(30.0)),
                    ..default()
                },
            ));

            // Choice buttons
            spawn_choice_button(panel, AttackChoice::Lead, "[LEAD]", "Rally the townspeople and fight");
            spawn_choice_button(panel, AttackChoice::Defend, "[DEFEND]", "Hold the gates, protect the weak");
            spawn_choice_button(panel, AttackChoice::Run, "[RUN]", "Flee into the forest");
            spawn_choice_button(panel, AttackChoice::Cower, "[COWER]", "Hide and pray it ends");
        });
    });
}

fn spawn_choice_button(parent: &mut ChildBuilder, choice: AttackChoice, label: &str, description: &str) {
    parent.spawn((
        Button,
        Node {
            padding: UiRect::axes(Val::Px(15.0), Val::Px(10.0)),
            margin: UiRect::bottom(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.2, 0.18, 0.12, 0.8)),
        BorderColor(Color::srgb(0.4, 0.35, 0.25)),
        ChoiceButton(choice),
    )).with_children(|btn| {
        btn.spawn((
            Text::new(format!("{:<10} {}", label, description)),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.75, 0.6)),
        ));
    });
}

fn handle_choice_buttons(
    mut state: ResMut<OpeningEventState>,
    mut interaction_q: Query<
        (&Interaction, &ChoiceButton, &mut BackgroundColor),
        Changed<Interaction>,
    >,
    mut complete_events: EventWriter<OpeningEventComplete>,
) {
    if state.phase != OpeningPhase::ShowingChoices {
        return;
    }

    for (interaction, choice_btn, mut bg_color) in interaction_q.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                let (origin, fame) = match choice_btn.0 {
                    AttackChoice::Lead => (HeroOrigin::Leader, 10),
                    AttackChoice::Defend => (HeroOrigin::Defender, 5),
                    AttackChoice::Run => (HeroOrigin::Wanderer, 0),
                    AttackChoice::Cower => (HeroOrigin::Survivor, 0),
                };

                state.origin = Some(origin);
                state.fame_local += fame;
                state.phase = OpeningPhase::FadingOut;
                state.fade_timer = 0.0;

                complete_events.send(OpeningEventComplete {
                    origin,
                    fame_gained: fame,
                });
            }
            Interaction::Hovered => {
                bg_color.0 = Color::srgba(0.35, 0.3, 0.2, 0.9);
            }
            Interaction::None => {
                bg_color.0 = Color::srgba(0.2, 0.18, 0.12, 0.8);
            }
        }
    }
}
