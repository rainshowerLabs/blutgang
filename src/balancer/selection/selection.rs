use crate::Rpc;

pub fn pick(list: &mut Vec<Rpc>) -> (Rpc, usize) {
    // If len is 1, return the only element
    if list.len() == 1 {
        return (list[0].clone(), 0);
    }

    algo(list)
}

#[cfg(all(feature = "selection-weighed-round-robin", not(feature = "selection-random")))]
fn algo(list: &mut Vec<Rpc>) -> (Rpc, usize) {
    // Sort by latency
    list.sort_by(|a, b| a.status.latency.partial_cmp(&b.status.latency).unwrap());

    if list[0].max_consecutive <= list[0].consecutive {
        list[1].consecutive = 1;
        list[0].consecutive = 0;
        return (list[1].clone(), 1);
    }

    list[0].consecutive += 1;
    (list[0].clone(), 0)
}

#[cfg(all(feature = "selection-weighed-round-robin", feature = "selection-random"))]
fn algo (list: &mut Vec<Rpc>) -> (Rpc, usize) {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..list.len());
    (list[index].clone(), index)
}
