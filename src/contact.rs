use crate::prelude::*;

enum Contacts {
    ShipAsteroid(Entity, Entity),
    LaserAsteroid(Entity, Entity),
}

pub struct ContactPlugin;

impl Plugin for ContactPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_set(
            SystemSet::on_update(AppState::Game)
                .with_system(contact_system.label(CanDespawnLaserLabel)),
        );
    }
}

pub fn contact_system(
    mut commands: Commands,
    mut gamestate: ResMut<State<AppGameState>>,
    mut asteroid_spawn_events: EventWriter<AsteroidSpawnEvent>,
    mut explosion_spawn_events: EventWriter<SpawnExplosionEvent>,
    mut laser_despawn_events: EventWriter<LaserDespawnEvent>,
    mut arena: ResMut<Arena>,
    mut events: EventReader<CollisionEvent>,
    damages: Query<&Damage>,
    mut ships: Query<(&Transform, &mut Ship)>,
    lasers: Query<(&Transform, &Laser)>,
    asteroids: Query<(&Velocity, &Transform, &Asteroid)>,
) {
    let mut contacts = vec![];
    for event in events.iter() {
        if let CollisionEvent::Started(e1, e2, _flags) = event {
            if ships.get_component::<Ship>(*e1).is_ok()
                && damages.get_component::<Damage>(*e2).is_ok()
            {
                contacts.push(Contacts::ShipAsteroid(*e1, *e2));
            }
            if ships.get_component::<Ship>(*e2).is_ok()
                && damages.get_component::<Damage>(*e1).is_ok()
            {
                contacts.push(Contacts::ShipAsteroid(*e2, *e1));
            }
            if asteroids.get_component::<Asteroid>(*e2).is_ok()
                && lasers.get_component::<Laser>(*e1).is_ok()
            {
                contacts.push(Contacts::LaserAsteroid(*e1, *e2));
            }
            if asteroids.get_component::<Asteroid>(*e1).is_ok()
                && lasers.get_component::<Laser>(*e2).is_ok()
            {
                contacts.push(Contacts::LaserAsteroid(*e2, *e1));
            }
        }
    }
    for contact in contacts.into_iter() {
        match contact {
            Contacts::LaserAsteroid(e1, e2) => {
                let laser_transform = lasers.get_component::<Transform>(e1).unwrap();
                let asteroid = asteroids.get_component::<Asteroid>(e2).unwrap();
                let asteroid_transform = asteroids.get_component::<Transform>(e2).unwrap();
                let asteroid_velocity = asteroids.get_component::<Velocity>(e2).unwrap();
                arena.score += asteroid.size.score();
                {
                    explosion_spawn_events.send(SpawnExplosionEvent {
                        kind: ExplosionKind::LaserOnAsteroid,
                        x: laser_transform.translation.x,
                        y: laser_transform.translation.y,
                    });
                    if let Some((size, radius)) = asteroid.size.split() {
                        let mut rng = thread_rng();
                        for _ in 0..rng.gen_range(1..4u8) {
                            let x =
                                asteroid_transform.translation.x + rng.gen_range(-radius..radius);
                            let y =
                                asteroid_transform.translation.y + rng.gen_range(-radius..radius);
                            let vx = rng.gen_range((-ARENA_WIDTH / radius)..(ARENA_WIDTH / radius));
                            let vy =
                                rng.gen_range((-ARENA_HEIGHT / radius)..(ARENA_HEIGHT / radius));
                            asteroid_spawn_events.send(AsteroidSpawnEvent {
                                size,
                                x,
                                y,
                                vx,
                                vy,
                                angvel: asteroid_velocity.angvel,
                            });
                        }
                    }
                }
                laser_despawn_events.send(LaserDespawnEvent(e1));
                commands.entity(e2).despawn();
            }
            Contacts::ShipAsteroid(e1, e2) => {
                let player_translation = ships.get_component::<Transform>(e1).unwrap().translation;
                let mut ship = ships.get_component_mut::<Ship>(e1).unwrap();
                let damage = damages.get_component::<Damage>(e2).unwrap();
                if ship.life > damage.value {
                    ship.life -= damage.value;
                } else {
                    ship.life = 0;
                }
                if ship.life == 0 {
                    explosion_spawn_events.send(SpawnExplosionEvent {
                        kind: ExplosionKind::ShipDead,
                        x: player_translation.x,
                        y: player_translation.y,
                    });
                    commands.entity(e1).despawn_recursive();
                    gamestate.set(AppGameState::GameOver).unwrap();
                } else {
                    explosion_spawn_events.send(SpawnExplosionEvent {
                        kind: ExplosionKind::ShipContact,
                        x: player_translation.x,
                        y: player_translation.y,
                    });
                }
            }
        }
    }
}
