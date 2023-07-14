use std::fmt::{self, Debug, Display, Formatter};

use crate::execution::*;

impl<Op, Ret> Display for Execution<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "INIT PART:")?;
        writeln!(f, "{}", &self.init_part)?;

        writeln!(f, "PARALLEL PART:")?;
        writeln!(f, "{}", self.parallel_part)?;

        writeln!(f, "POST PART:")?;
        writeln!(f, "{}", &self.post_part)?;

        Ok(())
    }
}

struct Column {
    header: String,
    spans: Vec<CellsSpan>,
}

struct Table {
    cell_height: usize,
    columns: Vec<Column>,
}

struct CellsSpan {
    len_in_cells: usize,
    content: Option<String>,
}

impl<Op, Ret> Display for Invocation<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(format!("{:?} : {:?}", self.op, self.ret).as_str())
    }
}

impl<Op, Ret> Display for ParallelInvocation<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(format!("{:?} : {:?}", self.op, self.ret).as_str())
    }
}

impl<Op, Ret> Display for History<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let spans: Vec<_> = self
            .iter()
            .map(|inv| CellsSpan::new(2, Some(format!("{}", inv))))
            .collect();

        let table = Table {
            cell_height: 2,
            columns: vec![Column {
                header: "MAIN THREAD".to_string(),
                spans,
            }],
        };

        table.fmt(f)
    }
}

// TODO: write tests for this
impl<Op, Ret> Display for ParallelHistory<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let thread_parts = self.get_thread_parts();
        let max_return_timestamp = thread_parts
            .iter()
            .filter_map(|thread_part| thread_part.last())
            .map(|&inv| inv.return_timestamp)
            .max()
            .unwrap_or(0);
        let max_return_timestamp = max_return_timestamp as isize;

        let columns: Vec<_> = thread_parts
            .into_iter()
            .enumerate()
            .map(|(thread_id, thread_part)| {
                let mut spans = Vec::new();
                let mut prev_inv_return_timestamp: isize = -1;

                for inv in thread_part {
                    let call_timestamp = inv.call_timestamp as isize;

                    if call_timestamp > prev_inv_return_timestamp + 1 {
                        spans.push(CellsSpan::new(
                            (call_timestamp - prev_inv_return_timestamp - 1) as usize,
                            None,
                        ));
                    }

                    spans.push(CellsSpan::new(
                        inv.return_timestamp - inv.call_timestamp + 1,
                        Some(format!("{}", inv)),
                    ));

                    prev_inv_return_timestamp = inv.return_timestamp as isize;
                }

                if prev_inv_return_timestamp < max_return_timestamp {
                    spans.push(CellsSpan::new(
                        (max_return_timestamp - prev_inv_return_timestamp) as usize,
                        None,
                    ));
                }

                Column {
                    header: format!("THREAD {}", thread_id),
                    spans,
                }
            })
            .collect();

        let table = Table {
            cell_height: 2,
            columns,
        };

        table.fmt(f)
    }
}

impl CellsSpan {
    fn new(len_in_cells: usize, content: Option<String>) -> Self {
        assert!(len_in_cells > 0);
        if let Some(ref content) = content {
            assert!(
                content.lines().count() <= 1,
                "multi-line content not supported"
            );
        }
        Self {
            len_in_cells,
            content,
        }
    }
    fn len_in_lines(&self, cell_height: usize) -> usize {
        self.len_in_cells * cell_height - 1 // -1 for the separator
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanState {
    NextSpan,
    Separator,
    Span { remaining: usize },
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColumnState {
    current_span: usize,
    span_state: SpanState,
}

impl Display for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let column_widths: Vec<_> = self
            .columns
            .iter()
            .map(|col| {
                col.spans
                    .iter()
                    .map(|span: &CellsSpan| {
                        if let Some(ref str) = span.content {
                            str.len() + 2
                        } else {
                            0
                        }
                    })
                    .max()
                    .unwrap_or(0)
                    .max(col.header.len() + 2)
            })
            .collect();

        let mut column_states: Vec<_> = self
            .columns
            .iter()
            .map(|column| ColumnState {
                current_span: 0,
                span_state: if column.spans.is_empty() {
                    SpanState::Finished
                } else {
                    SpanState::NextSpan
                },
            })
            .collect();

        let print_header_separator = |f: &mut Formatter<'_>| {
            write!(f, "|")?;
            for width in &column_widths {
                write!(f, "{:=<width$}|", "", width = width)?;
            }
            writeln!(f)
        };

        let print_header_contents = |f: &mut Formatter<'_>| {
            write!(f, "|")?;
            for (col, column) in self.columns.iter().enumerate() {
                let width = column_widths[col];
                let header = &column.header;

                write!(f, "{:^width$}|", header, width = width)?;
            }
            writeln!(f)
        };

        print_header_separator(f)?;
        print_header_contents(f)?;
        print_header_separator(f)?;

        while column_states
            .iter()
            .any(|state| state.span_state != SpanState::Finished)
        {
            write!(f, "|")?;
            for col in 0..self.columns.len() {
                let column_state = &mut column_states[col];
                let column = &self.columns[col];
                let width = column_widths[col];

                loop {
                    match column_state.span_state {
                        SpanState::NextSpan => {
                            let span = &self.columns[col].spans[column_state.current_span];
                            column_state.span_state = SpanState::Span {
                                remaining: span.len_in_lines(self.cell_height),
                            };
                        }
                        SpanState::Separator => {
                            write!(f, "{:-^width$}", "", width = width)?;
                            column_state.current_span += 1;
                            if column_state.current_span == self.columns[col].spans.len() {
                                column_state.span_state = SpanState::Finished;
                            } else {
                                column_state.span_state = SpanState::NextSpan;
                            }
                            break;
                        }
                        SpanState::Span { ref mut remaining } => {
                            if *remaining == 0 {
                                column_state.span_state = SpanState::Separator;
                                continue;
                            }

                            let span = &column.spans[column_state.current_span];
                            let content =
                                if *remaining == (span.len_in_lines(self.cell_height) + 1) / 2 {
                                    span.content.as_deref().unwrap_or("")
                                } else {
                                    ""
                                };
                            write!(f, "{:^width$}", content, width = width)?;

                            *remaining -= 1;
                            break;
                        }
                        SpanState::Finished => {
                            write!(f, "{:width$}", "", width = width)?;
                            break;
                        }
                    }
                }
                write!(f, "|")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
