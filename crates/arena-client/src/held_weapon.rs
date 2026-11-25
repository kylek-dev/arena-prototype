//! The Gladiator's two-handed sword, held in the right-hand socket of every
//! finished kit (ADR 0053), or carried on the back when the local player
//! sheathes it. The weapon GLB is built in the hand socket's own frame, so it
//! is parented there with an identity transform and follows every clip.
//! Presentation only: no combat rules.
use bevy::prelude::*;

use crate::appearance::KitInstance;

/// Built by scripts/finished_kit/weapon_build.py, shipped by prepare_weapon.py.
pub const PATH: &str = "weapons/gladiator-greatsword.glb";
/// The kit node that holds it drawn: +Y along the grip towards the blade.
pub const SOCKET: &str = "socket_weapon_r";
/// The kit node it rests on when sheathed: the model's axes at the upper back.
pub const BACK_SOCKET: &str = "socket_back";
/// Sheathed tilt from vertical: hilt over the right shoulder, tip towards the left hip.
const BACK_TILT_DEGREES: f32 = 25.0;
/// Where the grip sits from the back socket (glTF axes: +X the character's left,
/// +Y up, +Z front): right of the spine, above the shoulder blades, clear of the back.
const BACK_GRIP: Vec3 = Vec3::new(-0.12, 0.20, -0.055);

/// The local player's sheath state, from the browser's sheath keybind.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sheathed(pub bool);

/// A held weapon scene and the two sockets of its kit it moves between.
#[derive(Component)]
pub struct HeldWeapon {
    actor: usize,
    hand: Entity,
    back: Option<Entity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carry {
    Hand,
    Back,
}

/// Only the local player (actor 0) can sheathe: other fighters' sheath state
/// is not sent over the network, so they hold the sword drawn.
pub fn carry(actor: usize, sheathed: bool, has_back: bool) -> Carry {
    if actor == 0 && sheathed && has_back {
        Carry::Back
    } else {
        Carry::Hand
    }
}

/// The sword's transform in the back socket: its flat against the back.
pub fn back_transform() -> Transform {
    let (s, c) = BACK_TILT_DEGREES.to_radians().sin_cos();
    let tip = Vec3::new(s, -c, 0.0); // sword +Y: down and towards the character's left
    let flat = Vec3::NEG_Z; // sword +X: out of the back
    let edge = flat.cross(tip); // sword +Z
    Transform::from_translation(BACK_GRIP)
        .with_rotation(Quat::from_mat3(&Mat3::from_cols(flat, tip, edge)))
}

pub fn read(mut sheathed: ResMut<Sheathed>) {
    sheathed.set_if_neq(Sheathed(crate::platform::read_sheathed()));
}

/// Sockets that still need a weapon: named `SOCKET` and holding none yet.
pub fn sockets_to_fill<'a>(
    sockets: impl IntoIterator<Item = (Entity, &'a str)>,
    holding: &[Entity],
) -> Vec<Entity> {
    sockets
        .into_iter()
        .filter(|(entity, name)| *name == SOCKET && !holding.contains(entity))
        .map(|(entity, _)| entity)
        .collect()
}

/// Give every ready kit instance its sword, once, starting in the hand.
pub fn attach(
    mut commands: Commands,
    server: Res<AssetServer>,
    instances: Query<(Entity, &KitInstance)>,
    children: Query<&Children>,
    names: Query<&Name>,
    held: Query<&HeldWeapon>,
    mut scene: Local<Option<Handle<WorldAsset>>>,
) {
    let holding: Vec<Entity> = held.iter().map(|weapon| weapon.hand).collect();
    for (root, instance) in &instances {
        if !instance.ready {
            continue;
        }
        let sockets: Vec<(Entity, &str)> = children
            .iter_descendants(root)
            .filter_map(|entity| names.get(entity).ok().map(|name| (entity, name.as_str())))
            .collect();
        let back = sockets
            .iter()
            .find(|(_, name)| *name == BACK_SOCKET)
            .map(|(e, _)| *e);
        for hand in sockets_to_fill(sockets.iter().copied(), &holding) {
            let handle = scene
                .get_or_insert_with(|| server.load(GltfAssetLabel::Scene(0).from_asset(PATH)))
                .clone();
            commands.entity(hand).with_children(|parent| {
                parent.spawn((
                    WorldAssetRoot(handle),
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    HeldWeapon {
                        actor: instance.actor,
                        hand,
                        back,
                    },
                ));
            });
        }
    }
}

/// Move each sword to the hand or the back as its fighter's sheath state says.
pub fn place(
    mut commands: Commands,
    sheathed: Res<Sheathed>,
    mut weapons: Query<(Entity, &HeldWeapon, &ChildOf, &mut Transform)>,
) {
    for (entity, weapon, parent, mut transform) in &mut weapons {
        let (socket, wanted) = match carry(weapon.actor, sheathed.0, weapon.back.is_some()) {
            Carry::Back => (weapon.back.unwrap_or(weapon.hand), back_transform()),
            Carry::Hand => (weapon.hand, Transform::IDENTITY),
        };
        if parent.parent() != socket {
            commands.entity(entity).insert(ChildOf(socket));
        }
        if *transform != wanted {
            *transform = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_empty_right_hand_sockets_get_a_sword() {
        let mut world = World::new();
        let [right, left, full] = [(); 3].map(|_| world.spawn_empty().id());
        let sockets = [(right, SOCKET), (left, "socket_weapon_l"), (full, SOCKET)];
        assert_eq!(sockets_to_fill(sockets, &[full]), vec![right]);
        assert!(sockets_to_fill([(left, "socket_weapon_l")], &[]).is_empty());
    }

    #[test]
    fn only_the_local_player_sheathes_and_only_with_a_back_socket() {
        assert_eq!(carry(0, true, true), Carry::Back);
        assert_eq!(carry(0, false, true), Carry::Hand);
        assert_eq!(carry(0, true, false), Carry::Hand);
        assert_eq!(carry(3, true, true), Carry::Hand);
    }

    #[test]
    fn sheathed_sword_hangs_from_the_right_shoulder_to_the_left_hip() {
        let t = back_transform();
        let tip = t.transform_point(Vec3::Y); // a point towards the tip
        assert!(tip.y < t.translation.y && tip.x > t.translation.x, "{tip}");
        assert!(t.translation.x < 0.0 && t.translation.z < 0.0); // right of the spine, behind the back
        let m = Mat3::from_quat(t.rotation);
        assert!((m.determinant() - 1.0).abs() < 1e-5);
        assert!((m.x_axis - Vec3::NEG_Z).length() < 1e-5); // the flat faces away from the back
    }
}
