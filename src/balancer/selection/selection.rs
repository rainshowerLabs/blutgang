use crate::Rpc;

// Generic entry point fn to select the next rpc and return its position
pub fn pick(list: &mut Vec<Rpc>) -> (Rpc, Option<usize>) {
    // If len is 1, return the only element
    if list.len() == 1 {
        return (list[0].clone(), Some(0));
    } else if list.is_empty() {
        return (Rpc::default(), None);
    }

    algo(list)
}

// Selection algorithms
//
// Selected via features. selection-weighed-round-robin is a default feature.
// In order to have custom algos, you must add and enable the feature,
// as well as modify the cfg of the default algo to accomodate your new feature.
//

#[cfg(all(
    feature = "selection-weighed-round-robin",
    not(feature = "selection-random")
))]
fn algo(list: &mut [Rpc]) -> (Rpc, Option<usize>) {
    // Sort by latency
    list.sort_by(|a, b| a.status.latency.partial_cmp(&b.status.latency).unwrap());

    // Picks the second fastest one if the fastest one has maxed out
    if list[0].max_consecutive <= list[0].consecutive {
        list[1].consecutive = 1;
        list[0].consecutive = 0;
        return (list[1].clone(), Some(1));
    }

    list[0].consecutive += 1;
    (list[0].clone(), Some(0))
}

#[cfg(all(
    feature = "selection-weighed-round-robin",
    feature = "selection-random"
))]
fn algo(list: &mut [Rpc]) -> (Rpc, Option<usize>) {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..list.len());
    (list[index].clone(), Some(index))
}
