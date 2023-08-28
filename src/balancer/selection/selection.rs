use crate::Rpc;

// TODO: make this generic
pub fn pick(list: &mut Vec<Rpc>) -> (Rpc, usize) {
    // If len is 1, return the only element
    if list.len() == 1 {
        return (list[0].clone(), 0);
    }

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
