// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub index: usize,
    pub top: f64,
    pub bottom: f64,
    pub column: usize,
    pub columns: usize,
}

pub fn place(spans: &[(f64, f64)], minimum: f64, length: f64) -> Vec<Placed> {
    let mut placed: Vec<Placed> = spans
        .iter()
        .enumerate()
        .map(|(index, &(start, end))| {
            let top = start.min(length - minimum);
            let bottom = end.max(top + minimum);
            Placed {
                index,
                top,
                bottom,
                column: 0,
                columns: 1,
            }
        })
        .collect();
    placed.sort_by(|a, b| a.top.total_cmp(&b.top).then(b.bottom.total_cmp(&a.bottom)));

    let mut column_ends: Vec<f64> = Vec::new();
    let mut cluster_start = 0;
    let mut cluster_end = f64::NEG_INFINITY;
    for i in 0..placed.len() {
        if placed[i].top >= cluster_end {
            let columns = column_ends.len().max(1);
            placed[cluster_start..i]
                .iter_mut()
                .for_each(|item| item.columns = columns);
            column_ends.clear();
            cluster_start = i;
        }
        match column_ends.iter().position(|&end| end <= placed[i].top) {
            Some(free) => {
                placed[i].column = free;
                column_ends[free] = placed[i].bottom;
            }
            None => {
                placed[i].column = column_ends.len();
                column_ends.push(placed[i].bottom);
            }
        }
        cluster_end = cluster_end.max(placed[i].bottom);
    }
    let columns = column_ends.len().max(1);
    let total = placed.len();
    placed[cluster_start..total]
        .iter_mut()
        .for_each(|item| item.columns = columns);
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlaps_share_a_cluster_and_separate_events_stand_alone() {
        let placed = place(
            &[(10.0, 30.0), (20.0, 40.0), (25.0, 28.0), (50.0, 60.0)],
            1.0,
            400.0,
        );
        let by_index = |i| placed.iter().find(|p| p.index == i).unwrap();
        assert_eq!((by_index(0).column, by_index(0).columns), (0, 3));
        assert_eq!((by_index(1).column, by_index(1).columns), (1, 3));
        assert_eq!((by_index(2).column, by_index(2).columns), (2, 3));
        assert_eq!((by_index(3).column, by_index(3).columns), (0, 1));
    }

    #[test]
    fn a_short_event_gets_the_minimum_height_and_a_late_one_stays_inside() {
        let placed = place(&[(100.0, 100.0), (399.5, 400.0)], 6.0, 400.0);
        assert_eq!((placed[0].top, placed[0].bottom), (100.0, 106.0));
        assert_eq!((placed[1].top, placed[1].bottom), (394.0, 400.0));
    }

    #[test]
    fn a_freed_column_is_reused() {
        let placed = place(&[(0.0, 10.0), (5.0, 20.0), (12.0, 18.0)], 1.0, 400.0);
        let third = placed.iter().find(|p| p.index == 2).unwrap();
        assert_eq!((third.column, third.columns), (0, 2));
    }
}
