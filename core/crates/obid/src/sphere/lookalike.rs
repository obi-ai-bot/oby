//! Owner ids that could be mistaken for each other (docs/contracts/ask-an-oby.md, ASK-34 to 36):
//! whoever redeems an open invitation picks their own, so `dirna` can join beside `dima`. The rule is
//! small and fixed, written once here: no Unicode confusables table.

/// Every id in `others` that looks like `id`, sorted, once each, never `id` itself.
pub fn of<'a>(id: &str, others: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut alike: Vec<String> = others
        .into_iter()
        .filter(|other| *other != id && alike(id, other))
        .map(str::to_owned)
        .collect();
    alike.sort();
    alike.dedup();
    alike
}

/// Who a Confirm says the question doesn't go to: `dima's`, `a's or b's`, `a's, b's or c's`.
pub fn not_them(alike: &[String]) -> String {
    let theirs: Vec<String> = alike.iter().map(|id| format!("{id}'s")).collect();
    match theirs.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Two ids look alike when their skeletons match or they are one edit apart, case aside.
fn alike(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    skeleton(&a) == skeleton(&b) || one_edit(&a, &b)
}

/// A lowercased id without separators, `rn` as `m`, `l`/`1`/`i` as one letter and `0`/`o` as one.
fn skeleton(lower: &str) -> String {
    let joined: String = lower
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | '.'))
        .collect();
    joined
        .replace("rn", "m")
        .chars()
        .map(|c| match c {
            '1' | 'i' => 'l',
            '0' => 'o',
            c => c,
        })
        .collect()
}

/// Whether one character inserted, deleted or substituted turns one into the other.
fn one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let same = short.iter().zip(&long).take_while(|(x, y)| x == y).count();
    match long.len() - short.len() {
        0 => same < short.len() && short[same + 1..] == long[same + 1..],
        1 => short[same..] == long[same + 1..],
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alike_to(id: &str, others: &[&str]) -> Vec<String> {
        of(id, others.iter().copied())
    }

    /// ASK-36: an owner obid pinned once counts, in no directory.
    #[test]
    fn ask_36_a_pinned_id_counts() {
        let home = std::env::temp_dir().join(format!("obid-ask-36-{}", uuid::Uuid::new_v4()));
        let pins = super::super::pins::Pins::open(&home);
        let key = obi_protocol::keys::Keypair::from_seed(&[3; 32]).public_key();
        assert!(pins.check("dima", &key, 10));
        let pinned = pins.owners();
        assert_eq!(of("d1ma", pinned.iter().map(String::as_str)), ["dima"]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn lookalike_folds() {
        // Case.
        assert_eq!(alike_to("Dima", &["dima"]), ["dima"]);
        // rn and m.
        assert_eq!(alike_to("dirna", &["dima"]), ["dima"]);
        assert_eq!(alike_to("sarnuel", &["samuel"]), ["samuel"]);
        // l, 1 and I.
        assert_eq!(alike_to("wi11iam", &["wllliam"]), ["wllliam"]);
        assert_eq!(alike_to("ALICE", &["a1ice"]), ["a1ice"]);
        // 0 and o.
        assert_eq!(alike_to("r00ney", &["rooney"]), ["rooney"]);
        // Separators.
        assert_eq!(
            alike_to("sam-lee", &["sam_lee", "s.amlee"]),
            ["s.amlee", "sam_lee"]
        );
    }

    #[test]
    fn lookalike_one_edit() {
        // Inserted, deleted, substituted.
        assert_eq!(alike_to("ernesto", &["ernestoo"]), ["ernestoo"]);
        assert_eq!(alike_to("ernesto", &["ernsto"]), ["ernsto"]);
        assert_eq!(alike_to("ernesto", &["ernasto"]), ["ernasto"]);
        // In Unicode scalar values, case aside.
        assert_eq!(alike_to("jos\u{e9}", &["JOSE"]), ["JOSE"]);
        // Two edits aren't one.
        assert!(alike_to("ernesto", &["ermasto", "ernestooo"]).is_empty());
    }

    #[test]
    fn lookalike_leaves_out_who_doesnt_look_alike() {
        assert!(alike_to("ernesto", &["giulia"]).is_empty());
        assert!(alike_to("dima", &["dmytro"]).is_empty());
        assert!(alike_to("wellington", &["ernesto", "aaron", "sam", "copycat"]).is_empty());
    }

    #[test]
    fn lookalike_never_the_id_itself_and_each_once_sorted() {
        assert!(alike_to("dima", &["dima"]).is_empty());
        assert_eq!(
            alike_to("dima", &["dirna", "d1ma", "dirna", "dima"]),
            ["d1ma", "dirna"]
        );
    }

    #[test]
    fn lookalike_names_who_it_isnt() {
        let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        assert_eq!(not_them(&ids(&["dima"])), "dima's");
        assert_eq!(not_them(&ids(&["a", "b"])), "a's or b's");
        assert_eq!(not_them(&ids(&["a", "b", "c"])), "a's, b's or c's");
    }
}
