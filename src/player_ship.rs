use std::time::Duration;

use crate::prelude::*;

pub const START_LIFE: u32 = 3;
const INVINCIBLE_TIME: f32 = 2.0;
const MAX_INVINCIBLE_TIME: f32 = 5.0;

// Actions are divided in two enums
// One for pure Player Ship actions, during effective gameplay, added on the player entity itself.
// One for Menu actions, added as a global resource
#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, TypePath)]
pub enum PlayerAction {
    Forward,
    RotateLeft,
    RotateRight,
    Fire,
}

#[derive(Component)]
pub struct Ship {
    /// Ship rotation speed in rad/s
    pub rotation_speed: f32,
    /// Ship thrust N
    pub thrust: f32,
    /// Ship life points
    pub life: u32,
    /// Cannon auto-fire timer
    pub cannon_timer: Timer,
    /// Id of the controlling player. 1 or 2
    pub player_id: u32,
    // Timer triggered after being hit providing short-term invincibility
    pub invincible_timer: Timer,
    // Total duration of invincibility, accumulating when renewed
    pub invincible_time_secs: f32,
}

#[derive(Component, Clone, Copy)]
pub struct Damage {
    pub value: u32,
}

#[derive(Event)]
pub struct ShipAsteroidContactEvent {
    pub ship: Entity,
    pub asteroid: Entity,
}

pub struct PlayerShipPlugin;

impl Plugin for PlayerShipPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<PlayerAction>::default());
        app.add_event::<ShipAsteroidContactEvent>()
            .add_systems(OnEnter(AppState::GameCreate), spawn_ship)
            .add_systems(
                Update,
                (
                    ship_input_system,
                    ship_dampening_system,
                    ship_timers_system,
                    ship_invincible_color,
                    ship_damage.after(ContactSet),
                )
                    .run_if(in_state(AppState::GameRunning)),
            );
    }
}

// Tag component to update the exhaust particle effect with speed.
#[derive(Component)]
pub struct ExhaustEffect;

fn spawn_ship(mut commands: Commands, handles: Res<SpriteAssets>) {
    // For player actions, allow keyboard WASD/ Arrows/ Gamepag to control the ship
    let mut input_map = InputMap::new([
        (KeyCode::W, PlayerAction::Forward),
        (KeyCode::Up, PlayerAction::Forward),
        (KeyCode::A, PlayerAction::RotateLeft),
        (KeyCode::Left, PlayerAction::RotateLeft),
        (KeyCode::D, PlayerAction::RotateRight),
        (KeyCode::Right, PlayerAction::RotateRight),
        (KeyCode::Space, PlayerAction::Fire),
    ]);
    input_map.insert(GamepadButtonType::South, PlayerAction::Fire);
    input_map.insert(
        SingleAxis::positive_only(GamepadAxisType::LeftStickY, 0.4),
        PlayerAction::Forward,
    );
    input_map.insert(
        SingleAxis::negative_only(GamepadAxisType::LeftStickY, -0.4),
        PlayerAction::Forward,
    );
    input_map.insert(
        SingleAxis::positive_only(GamepadAxisType::LeftStickX, 0.4),
        PlayerAction::RotateRight,
    );
    input_map.insert(
        SingleAxis::negative_only(GamepadAxisType::LeftStickX, -0.4),
        PlayerAction::RotateLeft,
    );
    let mut invincible_timer = Timer::from_seconds(INVINCIBLE_TIME, TimerMode::Once);
    // Straghtaway consume the timer, we don't want invincibility at creation.
    invincible_timer.tick(Duration::from_secs_f32(INVINCIBLE_TIME));

    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                custom_size: Some(Vec2::new(30., 20.)),
                ..default()
            },
            transform: Transform {
                translation: Vec3::new(0.0, 0.0, 1.0),
                ..default()
            },
            texture: handles.player_ship.clone(),
            ..default()
        },
        Ship {
            rotation_speed: 3.0,
            thrust: 300000.0,
            life: START_LIFE,
            cannon_timer: Timer::from_seconds(0.2, TimerMode::Once),
            player_id: 1,
            invincible_timer,
            invincible_time_secs: 0.0,
        },
        ForState {
            states: AppState::ANY_GAME_STATE.to_vec(),
        },
        RigidBody::Dynamic,
        Collider::ball(13.5),
        ExternalForce::default(),
        LinearVelocity::ZERO,
        AngularVelocity::ZERO,
        InputManagerBundle::<PlayerAction> {
            action_state: ActionState::default(),
            input_map,
        },
    ));
}

fn ship_dampening_system(
    time: Res<Time>,
    mut query: Query<(&mut LinearVelocity, &mut AngularVelocity), With<Ship>>,
) {
    for (mut linvel, mut angvel) in query.iter_mut() {
        let elapsed = time.delta_seconds();
        angvel.0 *= 0.1f32.powf(elapsed);
        linvel.0 *= 0.4f32.powf(elapsed);
    }
}

fn ship_timers_system(time: Res<Time>, mut ship: Query<&mut Ship>) {
    for mut ship in ship.iter_mut() {
        ship.cannon_timer.tick(time.delta());
        ship.invincible_timer.tick(time.delta());
    }
}

#[allow(clippy::type_complexity)]
fn ship_input_system(
    mut laser_spawn_events: EventWriter<LaserSpawnEvent>,
    mut query: Query<(
        &ActionState<PlayerAction>,
        &mut ExternalForce,
        &mut LinearVelocity,
        &mut AngularVelocity,
        &Transform,
        &mut Ship,
    )>,
) {
    for (action_state, mut force, linvel, mut angvel, transform, mut ship) in query.iter_mut() {
        let thrust = if action_state.pressed(PlayerAction::Forward) {
            1.0
        } else {
            0.0
        };
        let rotation = if action_state.pressed(PlayerAction::RotateLeft) {
            1
        } else if action_state.pressed(PlayerAction::RotateRight) {
            -1
        } else {
            0
        };
        let fire = action_state.pressed(PlayerAction::Fire);
        if rotation != 0 {
            angvel.0 = rotation as f32 * ship.rotation_speed;
        }
        force.set_force((transform.rotation * (Vec3::Y * thrust * ship.thrust)).truncate());

        if fire && ship.cannon_timer.finished() {
            laser_spawn_events.send(LaserSpawnEvent {
                transform: *transform,
                linvel: *linvel,
            });
            ship.cannon_timer.reset();
        }
    }
}

fn ship_damage(
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut ship_asteroid_contact_events: EventReader<ShipAsteroidContactEvent>,
    mut explosion_spawn_events: EventWriter<SpawnExplosionEvent>,
    mut ships: Query<(&mut Ship, &Transform)>,
) {
    for event in ship_asteroid_contact_events.read() {
        let (mut ship, ship_transform) = ships
            .get_mut(event.ship)
            .expect("Ship referenced in event does not exist");
        if ship.invincible_timer.finished() {
            ship.invincible_time_secs = 0.0;
            ship.life -= 1;
            if ship.life == 0 {
                explosion_spawn_events.send(SpawnExplosionEvent {
                    kind: ExplosionKind::ShipDead,
                    x: ship_transform.translation.x,
                    y: ship_transform.translation.y,
                });
                commands.entity(event.ship).despawn_recursive();
                next_state.set(AppState::GameOver);
            } else {
                explosion_spawn_events.send(SpawnExplosionEvent {
                    kind: ExplosionKind::ShipContact,
                    x: ship_transform.translation.x,
                    y: ship_transform.translation.y,
                });
            }
            ship.invincible_timer.reset();
        } else if ship.invincible_time_secs + ship.invincible_timer.elapsed_secs()
            < MAX_INVINCIBLE_TIME
        {
            // Contact while invincible, rearm the invincibility time if allowed
            ship.invincible_time_secs += ship.invincible_timer.elapsed_secs();
            ship.invincible_timer.reset();
        }
    }
}

fn ship_invincible_color(mut ships: Query<(&Ship, &mut Sprite)>) {
    for (ship, mut ship_sprite) in ships.iter_mut() {
        if ship.invincible_timer.finished() {
            ship_sprite.color = Color::WHITE;
        } else {
            let alpha = (ship.invincible_timer.elapsed_secs() * 2.0) % 1.0;
            ship_sprite.color = Color::rgba(1.0, 0.4, 0.2, alpha);
        }
    }
}
