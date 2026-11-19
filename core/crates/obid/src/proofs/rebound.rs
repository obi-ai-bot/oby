//! A rebound join (docs/contracts/obysphere.md, SPHERE-24): with no invitation to redeem, obid takes a
//! membership only when the Sphere's directory lists this Oby active, under the body's owner and
//! owner key, its card carrying obid's own Obi key; every other combination joins nothing. Every
//! field any bytes up to 4 or none, the status any bytes up to 7, one past `active` (`any_bytes_in`),
//! or none, with no entry at all too.
//! What isn't proved here: that the entry read is the directory's for this Obi id (the first whose
//! `obi` is it) and that the Sphere took the delegation (`invalid_grant`), which `sphere_api`'s
//! tests cover.
//! Proves: core/crates/obid/src/sphere/link.rs

use super::any_bytes_in;
use crate::sphere::link::{Listed, Rebound, Unbound, rebound_refusal};

fn field(buf: &mut [u8; 4]) -> Option<&[u8]> {
    if kani::any() {
        Some(any_bytes_in(buf))
    } else {
        None
    }
}

#[kani::proof]
#[kani::unwind(9)]
fn sphere_24_a_rebound_join_takes_only_its_own_active_listing() {
    let mut ours_bufs = [[0u8; 4]; 4];
    let [b0, b1, b2, b3] = &mut ours_bufs;
    let ours = Rebound {
        owner: any_bytes_in(b0),
        owner_key: any_bytes_in(b1),
        key_id: any_bytes_in(b2),
        public_key: any_bytes_in(b3),
    };
    let mut status_buf = [0u8; 7];
    let status = if kani::any() {
        Some(any_bytes_in(&mut status_buf))
    } else {
        None
    };
    let mut listed_bufs = [[0u8; 4]; 4];
    let [l0, l1, l2, l3] = &mut listed_bufs;
    let entry = Listed {
        status,
        owner: field(l0),
        owner_key: field(l1),
        key_id: field(l2),
        public_key: field(l3),
    };
    let listed = kani::any::<bool>();
    let refusal = rebound_refusal(listed.then_some(entry), &ours);
    let joins = listed
        && entry.status == Some(b"active")
        && entry.owner == Some(ours.owner)
        && entry.owner_key == Some(ours.owner_key)
        && entry.key_id == Some(ours.key_id)
        && entry.public_key == Some(ours.public_key);
    assert_eq!(refusal.is_none(), joins);
    if !listed {
        assert_eq!(refusal, Some(Unbound::NotListed));
    }
}
