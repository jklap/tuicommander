use super::{
    fake::{InMemoryWorkItemProvider, WorkItemError},
    model::*,
    protocol::*,
};
use std::collections::BTreeMap;

impl InMemoryWorkItemProvider {
    pub fn create_fields(&self, _item_type: Option<&str>) -> Vec<WorkItemFieldSchema> {
        self.fields.clone()
    }

    pub fn grouped(&self, query: &WorkItemQuery) -> Result<WorkItemGroupedPage, WorkItemError> {
        let field = query
            .group_by
            .as_ref()
            .and_then(|id| self.fields.iter().find(|f| &f.id == id))
            .ok_or_else(|| WorkItemError::Query("unknown group field".into()))?;
        let page = self.list(query)?;
        let mut groups = BTreeMap::<String, WorkItemGroup>::new();
        for item in page.items {
            let mut keys = match item.fields.get(&field.id).and_then(Option::as_ref) {
                Some(WorkItemFieldValue::User(users)) => users
                    .iter()
                    .map(|u| (format!("user:{}", u.id), u.name.clone()))
                    .collect::<Vec<_>>(),
                Some(WorkItemFieldValue::Labels(labels)) => labels
                    .iter()
                    .map(|l| (format!("labels:{}", l.id), l.name.clone()))
                    .collect(),
                Some(WorkItemFieldValue::Enum(id)) => vec![(
                    format!("enum:{id}"),
                    field
                        .options
                        .iter()
                        .find(|o| &o.id == id)
                        .map_or_else(|| id.clone(), |o| o.name.clone()),
                )],
                Some(WorkItemFieldValue::Text(value)) => {
                    vec![(format!("text:{value}"), value.clone())]
                }
                Some(WorkItemFieldValue::Markdown(value)) => {
                    vec![(format!("markdown:{value}"), value.clone())]
                }
                Some(WorkItemFieldValue::Date(value)) => {
                    vec![(format!("date:{value}"), value.clone())]
                }
                Some(WorkItemFieldValue::Link(value)) => {
                    vec![(format!("link:{value}"), value.clone())]
                }
                Some(WorkItemFieldValue::Number(value)) => {
                    vec![(format!("number:{value}"), value.to_string())]
                }
                None => vec![],
            };
            if keys.is_empty() {
                keys.push(("unset".into(), "Unset".into()));
            }
            for (id, name) in keys {
                groups
                    .entry(id.clone())
                    .or_insert_with(|| WorkItemGroup {
                        id,
                        name,
                        items: vec![],
                    })
                    .items
                    .push(item.clone());
            }
        }
        Ok(WorkItemGroupedPage {
            groups: groups.into_values().collect(),
            next_cursor: page.next_cursor,
        })
    }

    pub fn board(
        &self,
        id: &str,
        query: &WorkItemQuery,
    ) -> Result<WorkItemBoardPage, WorkItemError> {
        let board = self
            .boards
            .iter()
            .find(|b| b.id == id)
            .ok_or_else(|| WorkItemError::Query("unknown board".into()))?;
        let page = self.list(query)?;
        let mut lanes: Vec<_> = board
            .columns
            .iter()
            .map(|c| WorkItemBoardLane {
                column_id: c.id.clone(),
                cards: vec![],
            })
            .collect();
        for item in page.items {
            let detail = self.detail(&item.reference)?;
            let moves = board
                .columns
                .iter()
                .filter(|c| legal(&detail, &c.action))
                .map(|c| WorkItemBoardMove {
                    column_id: c.id.clone(),
                    action: c.action.clone(),
                })
                .collect::<Vec<_>>();
            for (column, lane) in board.columns.iter().zip(&mut lanes) {
                let belongs = match &column.action {
                    WorkItemAction::EditField(edit) => match edit.value.as_ref() {
                        Some(value) => super::validation::matches_filter(
                            item.fields.get(&board.field_id).and_then(Option::as_ref),
                            value,
                        ),
                        None => item.fields.get(&board.field_id).is_none_or(Option::is_none),
                    },
                    WorkItemAction::Transition(id) => &item.state.id == id,
                    _ => false,
                };
                if belongs {
                    lane.cards.push(WorkItemBoardCard {
                        item: item.clone(),
                        moves: moves.clone(),
                    });
                }
            }
        }
        Ok(WorkItemBoardPage {
            board: board.clone(),
            lanes,
            next_cursor: page.next_cursor,
        })
    }
}

fn legal(detail: &WorkItemDetail, action: &WorkItemAction) -> bool {
    match action {
        WorkItemAction::EditField(edit) => detail.fields.iter().any(|field| {
            field.id == edit.field_id
                && field.editable
                && super::validation::validate_value(field, edit.value.as_ref()).is_ok()
        }),
        WorkItemAction::Transition(id) => detail
            .actions
            .iter()
            .any(|a| &a.id == id && a.kind == WorkItemActionKind::Transition && a.enabled),
        _ => false,
    }
}
