use super::*;
use enzui_widgets::scroll_view::enz_scrollable as scrollable;
use enzui_widgets::{drag_drop_host as drag, reorder_list as reorder};
use iced::widget::{Space, column, container, row, text};

/// How long after a tab opens its row still makes its entrance when it
/// first appears.
const TAB_ENTRANCE: std::time::Duration = std::time::Duration::from_millis(600);
/// In the icon rail a tab's row reaches this far past its tile to the sides
/// and above it, so its close and mute buttons stand over the tile's corners
/// rather than inside them, where a click meant for the tab lands.
const RAIL_REACH_X: f32 = 2.0;
const RAIL_REACH_Y: f32 = 4.0;

#[derive(Clone)]
struct TopTabPlan {
    width: f32,
    header: f32,
    icons: bool,
    visible: Vec<u64>,
    overflow: bool,
}
impl TopTabPlan {
    fn new(browser: &Browser) -> Self {
        let mut keys: Vec<u64> = browser
            .saved
            .tabs
            .iter()
            .filter(|t| t.pinned)
            .map(|t| t.key)
            .collect();
        let order = browser.strip_order();
        for e in &order {
            match e {
                TabStripEntry::Tab(k) => keys.push(*k),
                TabStripEntry::Group(g) => keys.extend(
                    browser
                        .saved
                        .tabs
                        .iter()
                        .filter(|t| !t.pinned && t.group == Some(*g))
                        .map(|t| t.key),
                ),
            }
        }
        let available = (browser.viewport.width - 54.).max(28.);
        let groups = browser.saved.groups.len() as f32;
        let n = keys.len().max(1) as f32;
        let pinned_overhead = if browser.saved.tabs.iter().any(|tab| tab.pinned) {
            13.
        } else {
            0.
        };
        let budget =
            |header: f32| (available - groups * (header + 8.) - n * 2. - pinned_overhead) / n;
        let header = 20.;
        let raw = budget(header);
        let overflow = raw < 28.;
        let width = raw.clamp(28., 196.);
        if overflow {
            let by_group: std::collections::BTreeMap<_, _> = browser
                .saved
                .tabs
                .iter()
                .filter(|t| !t.pinned)
                .filter_map(|t| t.group.map(|g| (t.key, g)))
                .collect();
            let mut capacity = ((available - 32.) / 30.).floor().max(1.) as usize;
            capacity = capacity.min(keys.len());
            let active = browser.saved.tabs[browser.saved.active].key;
            let index = keys.iter().position(|k| *k == active).unwrap_or(0);
            loop {
                let start = index
                    .saturating_sub(capacity / 2)
                    .min(keys.len().saturating_sub(capacity));
                let visible = &keys[start..start + capacity];
                let present: std::collections::BTreeSet<_> =
                    visible.iter().filter_map(|k| by_group.get(k)).collect();
                let footprint =
                    capacity as f32 * 30. + present.len() as f32 * 28. + pinned_overhead + 32.;
                if footprint <= available || capacity <= 1 {
                    keys = visible.to_vec();
                    break;
                }
                capacity -= 1;
            }
        }
        Self {
            width,
            header,
            icons: width < 42.,
            visible: keys,
            overflow,
        }
    }
    fn contains(&self, key: u64) -> bool {
        self.visible.contains(&key)
    }
}
fn tab_group_drop_bounds(size: iced::Size, horizontal: bool) -> iced::Rectangle {
    // Keep the outer quarters for insertion before/after the tab.
    if horizontal {
        iced::Rectangle::new(
            iced::Point::new(size.width * 0.25, 0.),
            iced::Size::new(size.width * 0.5, size.height),
        )
    } else {
        iced::Rectangle::new(
            iced::Point::new(0., size.height * 0.25),
            iced::Size::new(size.width, size.height * 0.5),
        )
    }
}
fn tab_group_drop_item(
    key: u64,
    size: iced::Size,
    horizontal: bool,
) -> enzui_widgets::drag_item_surface::DragSurfaceItem {
    enzui_widgets::drag_item_surface::DragSurfaceItem::new(
        enzui_core::drag::ItemId::new(format!("tab:{key}")),
        tab_group_drop_bounds(size, horizontal),
        "Group with this tab",
    )
    .drag_source(false)
    .drop_target(true)
    .feedback_bounds(iced::Rectangle::with_size(size))
}

fn drop_message(request: enzui_core::drag::DropRequest, visible: Option<Vec<u64>>) -> Message {
    c(match visible {
        Some(keys) => Command::DropVisibleTabs(request, keys),
        None => Command::DropTabs(request),
    })
}

impl App {
    pub(crate) fn effective_tab_layout(&self) -> Option<Layout> {
        if self.popup_window {
            return None;
        }
        let width = self.browser.viewport.width;
        match self.browser.saved.layout {
            Layout::Top if width >= 640.0 => Some(Layout::Top),
            Layout::Top => None,
            _ if width < 720.0 => None,
            Layout::Sidebar if width < 1000.0 => Some(Layout::Icons),
            layout => Some(layout),
        }
    }
    fn tabs_list(
        &self,
        id: &str,
        layout: Layout,
        channel: &drag::DragHostChannel,
        plan: Option<&TopTabPlan>,
    ) -> Element<'_, Message> {
        let items: Vec<_> = self
            .browser
            .saved
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| tab_collection(t) == id && plan.is_none_or(|p| p.contains(t.key)))
            .map(|(i, t)| {
                (
                    format!("tab:{}", t.key),
                    reorder::reorder_handle(self.entering_tab(i, layout, channel, plan)).into(),
                )
            })
            .collect();
        let empty = items.is_empty();
        let visible = plan.filter(|p| p.overflow).map(|p| p.visible.clone());
        let mut list = reorder::enz_reorder_list(items, move |r| drop_message(r, visible.clone()))
            .animate_changes()
            .collection_id(id.to_owned())
            .drag_host(channel)
            .drag_handles(true)
            .row_radius(7.)
            .spacing(2.)
            .width(if layout == Layout::Top {
                Length::Shrink
            } else {
                Length::Fill
            });
        if layout == Layout::Top {
            list = list.horizontal();
        }
        if empty {
            list = list.height(Length::Fixed(24.));
        }
        list.into_element()
    }
    fn tabs_header(
        &self,
        group: Option<usize>,
        layout: Layout,
        channel: &drag::DragHostChannel,
        plan: Option<&TopTabPlan>,
    ) -> Element<'_, Message> {
        use enzui_widgets::drag_item_surface::{DragSurfaceItem, enz_drag_item_surface};
        let top = layout == Layout::Top;
        let icons = layout == Layout::Icons || plan.is_some_and(|p| p.icons);
        let label = group.map_or_else(
            || "Pinned".to_owned(),
            |g| self.browser.saved.groups[g].name.clone(),
        );
        let color = group.map_or_else(
            || self.look().palette.text_muted,
            |g| {
                group_ink(
                    self.browser.saved.groups[g].color,
                    self.look().palette.is_dark(),
                )
            },
        );
        let header: Element<'_, Message> = if top {
            container(dot(color, 6.)).center_x(20).center_y(26).into()
        } else if icons {
            Space::new().width(Length::Fill).height(4).into()
        } else {
            let edit = group.map_or_else(
                || Space::new().width(0).into(),
                |g| {
                    ink_icon(
                        "ellipsis",
                        "Edit tab group",
                        c(Command::Panel(Panel::Group(g))),
                        20.,
                        12.,
                    )
                },
            );
            container(
                row![
                    dot(color, 6.),
                    text(label.clone())
                        .size(11)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .ellipsis(iced::widget::text::Ellipsis::End)
                        .width(Length::Fill),
                    edit
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .padding([2, 6])
            .height(26)
            .width(if top {
                Length::Fixed(plan.map_or(96., |p| p.header))
            } else {
                Length::Fill
            })
            .into()
        };
        let header: Element<'_, Message> = enzui_widgets::tooltip::FadeTooltip::new(
            header,
            label.clone(),
            iced::widget::tooltip::Position::Bottom,
        )
        .delay(std::time::Duration::from_millis(600))
        .into();
        let id = group.map_or_else(|| "pinned".into(), |g| format!("group:{g}"));
        let target = format!("target:{id}");
        let header = if group.is_some() {
            reorder::reorder_handle(header).into()
        } else {
            header
        };
        enz_drag_item_surface(
            channel,
            header,
            enzui_core::drag::CollectionId::new(format!("header:{id}")),
            vec![],
        )
        .items_from_size(move |size| {
            vec![
                DragSurfaceItem::new(
                    enzui_core::drag::ItemId::new(target.clone()),
                    iced::Rectangle::with_size(size),
                    label.clone(),
                )
                .drag_source(false)
                .drop_target(true),
            ]
        })
        .accepts_background(false)
        .destination_operations(vec![enzui_core::drag::DragOperation::Move])
        .into_element()
    }
    fn tab_group_block(
        &self,
        g: usize,
        layout: Layout,
        channel: &drag::DragHostChannel,
        plan: Option<&TopTabPlan>,
    ) -> Element<'_, Message> {
        let top = layout == Layout::Top;
        let header = self.tabs_header(Some(g), layout, channel, plan);
        let list = self.tabs_list(&format!("group:{g}"), layout, channel, plan);
        let block: Element<'_, Message> = if top {
            row![header, list]
                .spacing(4)
                .align_y(iced::Alignment::Center)
                .into()
        } else if layout == Layout::Icons {
            list
        } else {
            column![header, list].spacing(2).width(Length::Fill).into()
        };
        let color = group_ink(
            self.browser.saved.groups[g].color,
            self.look().palette.is_dark(),
        );
        let area: Element<'_, Message> = container(block)
            .padding(if top {
                iced::Padding::from([0, 2])
            } else if layout == Layout::Icons {
                // A rail row reaches past its tile above and to the sides;
                // that reach stands in for the group's padding there.
                iced::Padding {
                    top: 4.0 - RAIL_REACH_Y,
                    right: 2.0 - RAIL_REACH_X,
                    bottom: 4.0,
                    left: 2.0 - RAIL_REACH_X,
                }
            } else {
                iced::Padding::from(4)
            })
            .id(iced::widget::Id::from(format!("tab-group-{g}")))
            .style(move |_| container::Style {
                background: Some(color.scale_alpha(0.14).into()),
                border: iced::Border {
                    color: color.scale_alpha(0.25),
                    width: 1.,
                    radius: 9.into(),
                },
                ..Default::default()
            })
            .into();
        let area = if layout == Layout::Icons {
            use enzui_widgets::drag_item_surface::{DragSurfaceItem, enz_drag_item_surface};
            let label = self.browser.saved.groups[g].name.clone();
            let area = enz_drag_item_surface(
                channel,
                area,
                enzui_core::drag::CollectionId::new(format!("header:group:{g}")),
                vec![],
            )
            .items_from_size(move |size| {
                vec![
                    DragSurfaceItem::new(
                        enzui_core::drag::ItemId::new(format!("target:group:{g}")),
                        iced::Rectangle::with_size(iced::Size::new(size.width, 4.)),
                        label.clone(),
                    )
                    .drag_source(false)
                    .drop_target(true),
                ]
            })
            .accepts_background(false)
            .destination_operations(vec![enzui_core::drag::DragOperation::Move])
            .into_element();
            reorder::reorder_handle(area).into()
        } else {
            area
        };
        enzui_widgets::context_target::enz_context_target(area, move |point| {
            c(Command::GroupContext(g, point))
        })
        .width(if top { Length::Shrink } else { Length::Fill })
        .height(Length::Shrink)
        .into_element()
    }
    fn tab_drag_surface(&self, layout: Layout) -> Element<'_, Message> {
        let channel = drag::DragHostChannel::new();
        let top = layout == Layout::Top;
        let top_plan = top.then(|| TopTabPlan::new(&self.browser));
        let plan = top_plan.as_ref();
        let visible = plan.filter(|p| p.overflow).map(|p| p.visible.clone());
        let mut items = Vec::new();
        for entry in self.browser.strip_order() {
            match entry {
                TabStripEntry::Tab(key) => {
                    if let Some(i) = self.browser.saved.tabs.iter().position(|t| t.key == key)
                        && plan.is_none_or(|p| p.contains(key))
                    {
                        items.push((
                            format!("tab:{key}"),
                            reorder::reorder_handle(
                                self.entering_tab(i, layout, &channel, plan),
                            )
                            .into(),
                        ));
                    }
                }
                TabStripEntry::Group(g) => {
                    if plan.is_none_or(|p| {
                        !p.overflow
                            || self
                                .browser
                                .saved
                                .tabs
                                .iter()
                                .any(|t| t.group == Some(g) && p.contains(t.key))
                    }) {
                        items.push((
                            format!("group:{g}"),
                            self.tab_group_block(g, layout, &channel, plan),
                        ));
                    }
                }
            }
        }
        let list_visible = visible.clone();
        let mut main =
            reorder::enz_reorder_list(items, move |r| drop_message(r, list_visible.clone()))
                .animate_changes()
                .collection_id("main")
                .drag_host(&channel)
                .drag_handles(true)
                // In the rail a row's reach above its tile is part of the gap.
                .spacing(if top {
                    2.
                } else if layout == Layout::Icons {
                    6. - RAIL_REACH_Y
                } else {
                    6.
                })
                .row_radius(7.)
                .width(if top { Length::Shrink } else { Length::Fill });
        if top {
            main = main.horizontal();
        }
        let mut blocks: Vec<Element<'_, Message>> = Vec::new();
        if self
            .browser
            .saved
            .tabs
            .iter()
            .any(|t| t.pinned && plan.is_none_or(|p| p.contains(t.key)))
        {
            // The divider below marks the pinned tabs; they have no heading.
            blocks.push(self.tabs_list("pinned", layout, &channel, plan));
            blocks.push(if top {
                container(iced::widget::rule::vertical(1)).height(24).into()
            } else {
                container(iced::widget::rule::horizontal(1))
                    .padding([6, 4])
                    .into()
            });
        }
        if top {
            blocks.push(main.into_element());
        } else {
            // The pinned tabs stay put; the others scroll under them, without
            // a scrollbar. The scroll boundary keeps both lists in this one
            // host, so a tab still drags between them.
            blocks.push(
                scrollable(drag::drag_scroll_boundary(&channel, main.into_element()))
                    .direction(iced::widget::scrollable::Direction::Vertical(
                        iced::widget::scrollable::Scrollbar::hidden(),
                    ))
                    .height(Length::Fill)
                    .into(),
            );
        }
        let content: Element<'_, Message> = if top {
            row(blocks)
                .spacing(6)
                .align_y(iced::Alignment::Center)
                .into()
        } else {
            column(blocks)
                .spacing(4)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };
        let main_order = self.browser.strip_order();
        let host_visible = visible.clone();
        let hosted = drag::enz_drag_drop_host(&channel, content)
            .on_drop(move |r| drop_message(r, host_visible.clone()))
            .on_item_drop(move |drop| {
                let mut request = drop.request;
                if let Some(key) = request
                    .destination
                    .0
                    .strip_prefix("onto-tab:")
                    .and_then(|id| id.parse().ok())
                {
                    return c(Command::GroupDrop(request, key));
                }
                if let Some(id) = request.destination.0.strip_prefix("header:") {
                    if request.items.iter().any(|i| i.0.starts_with("group:")) {
                        let position = id
                            .strip_prefix("group:")
                            .and_then(|v| v.parse::<usize>().ok())
                            .and_then(|g| {
                                main_order
                                    .iter()
                                    .position(|e| *e == TabStripEntry::Group(g))
                            })
                            .unwrap_or(0);
                        request.destination = enzui_core::drag::CollectionId::new("main");
                        request.slot = enzui_core::drag::InsertionSlot(position);
                        return c(Command::DropTabs(request));
                    } else {
                        request.destination = enzui_core::drag::CollectionId::new(id);
                        request.slot = enzui_core::drag::InsertionSlot(usize::MAX);
                    }
                }
                drop_message(request, visible.clone())
            })
            .into_element();
        if top {
            hosted
        } else {
            iced::widget::mouse_area(hosted)
                .on_middle_press(c(Command::New))
                .into()
        }
    }
    pub(super) fn tab_strip_for(&self, layout: Layout) -> Element<'_, Message> {
        let top = layout == Layout::Top;
        let icons = layout == Layout::Icons;
        let top_plan = top.then(|| TopTabPlan::new(&self.browser));
        let add = if icons || top {
            icon("plus", "New tab", c(Command::New))
        } else {
            action("New tab", "plus", Command::New)
        };
        if top {
            let mut strip = row![
                container(self.tab_drag_surface(layout))
                    .width(Length::Fill)
                    .clip(true)
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center);
            if top_plan.as_ref().is_some_and(|p| p.overflow) {
                strip = strip.push(icon("chevron-down", "More tabs", c(Command::TopTabsMenu)));
            }
            iced::widget::mouse_area(
                strip
                    .push(add)
                    .height(self.chrome_metrics().tab - 2.)
                    .width(Length::Fill),
            )
                .on_middle_press(c(Command::New))
                .into()
        } else {
            let libraries = self.tab_library_actions(icons);
            let header: Element<'_, Message> = if icons {
                container(ink_icon("plus", "New tab", c(Command::New), 28., 16.))
                    .center_x(Length::Fill)
                    .into()
            } else {
                row![
                    libraries,
                    Space::new().width(Length::Fill),
                    ink_icon("plus", "New tab", c(Command::New), 28., 16.)
                ]
                .align_y(iced::Alignment::Center)
                .into()
            };
            let tabs = container(self.tab_drag_surface(layout)).height(Length::Fill);
            // In immersive view the library buttons sit at the sidebar's foot,
            // and the list's own New tab row stands for the header's button.
            let mut strip = if self.immersive() {
                column![tabs]
            } else {
                column![container(header).padding([2, 0]), tabs].spacing(6)
            };
            if icons {
                strip = strip.push(self.tab_library_actions(true));
            }
            let metrics = self.chrome_metrics();
            container(strip)
                .width(if icons { metrics.rail } else { metrics.sidebar })
                .height(Length::Fill)
                .padding(if icons {
                    iced::Padding::from([4, 2])
                } else {
                    iced::Padding::from(4)
                })
                .into()
        }
    }
    pub(super) fn tab_library_actions(&self, icons: bool) -> Element<'_, Message> {
        let entries = [
            (
                "bookmark",
                "Bookmarks",
                InternalPage::Bookmarks,
                Command::Panel(Panel::Bookmarks),
            ),
            (
                "history",
                "History",
                InternalPage::History,
                Command::Panel(Panel::History),
            ),
            (
                "container",
                "Containers",
                InternalPage::Containers,
                Command::Container(ContainerCommand::Open),
            ),
        ];
        let buttons = entries
            .into_iter()
            .map(|(symbol, label, page, command)| {
                enzui_widgets::toolbar_icon::enz_toolbar_icon(symbol, c(command))
                    .aria_label(label)
                    .tooltip(label)
                    .size(28., 16.)
                    .active(self.browser.library == Some(page))
                    .into_element()
            })
            .chain(std::iter::once(self.appearance_button()));
        if icons {
            column(buttons)
                .spacing(2)
                .align_x(iced::Alignment::Center)
                .width(Length::Fill)
                .into()
        } else {
            row(buttons)
                .spacing(2)
                .align_y(iced::Alignment::Center)
                .into()
        }
    }
    /// A tab's row, which a tab just opened enters: it slides in from the
    /// side it is listed along and fades in. The wrapper is always there, so
    /// a row keeps its state when its entrance ends; the entrance itself only
    /// plays when the row first appears.
    fn entering_tab(
        &self,
        i: usize,
        layout: Layout,
        channel: &drag::DragHostChannel,
        plan: Option<&TopTabPlan>,
    ) -> Element<'_, Message> {
        let row = enzui_widgets::motion::animated(self.tab_drop_target(i, layout, channel, plan));
        let entering = self
            .browser
            .tab_entrances
            .get(&self.browser.saved.tabs[i].key)
            .is_some_and(|at| at.elapsed() < TAB_ENTRANCE);
        if entering {
            let from = if layout == Layout::Top {
                iced::Vector::new(0.0, -8.0)
            } else {
                iced::Vector::new(-16.0, 0.0)
            };
            row.enters_from(from, 0.0).into()
        } else {
            row.into()
        }
    }
    fn tab_drop_target(
        &self,
        i: usize,
        layout: Layout,
        channel: &drag::DragHostChannel,
        plan: Option<&TopTabPlan>,
    ) -> Element<'_, Message> {
        let tab = &self.browser.saved.tabs[i];
        let content = self.tab_row(i, layout, plan);
        let content = if layout == Layout::Icons && tab.group.is_none() {
            container(container(content).width(self.chrome_metrics().rail - 8. + 2.0 * RAIL_REACH_X))
                .center_x(Length::Fill)
                .into()
        } else {
            content
        };
        if tab.pinned {
            return content;
        }
        let key = tab.key;
        enzui_widgets::drag_item_surface::enz_drag_item_surface(
            channel,
            content,
            enzui_core::drag::CollectionId::new(format!("onto-tab:{key}")),
            vec![],
        )
        .items_from_size(move |size| vec![tab_group_drop_item(key, size, layout == Layout::Top)])
        .accepts_background(false)
        .drop_target_highlight(self.look().palette.accent.scale_alpha(0.12))
        .drop_target_outline(self.look().palette.accent, 7.)
        .destination_operations(vec![enzui_core::drag::DragOperation::Move])
        .into_element()
    }
    pub(super) fn group_tab_drop(
        &mut self,
        mut request: enzui_core::drag::DropRequest,
        target: u64,
    ) {
        use enzui_core::drag::{CollectionId, DragOperation, InsertionSlot};
        if request.operation != DragOperation::Move || request.items.is_empty() {
            return;
        }
        let Some(target_index) = self
            .browser
            .saved
            .tabs
            .iter()
            .position(|tab| tab.key == target && !tab.pinned)
        else {
            return;
        };
        let mut keys = Vec::new();
        for item in &request.items {
            let Some(key) = item
                .0
                .strip_prefix("tab:")
                .and_then(|id| id.parse::<u64>().ok())
            else {
                return;
            };
            if key == target
                || keys.contains(&key)
                || !self
                    .browser
                    .saved
                    .tabs
                    .iter()
                    .any(|tab| tab.key == key && tab_collection(tab) == request.source.0)
            {
                return;
            }
            keys.push(key);
        }
        let group = if let Some(group) = self.browser.saved.tabs[target_index].group {
            group
        } else {
            let mut order = self.browser.strip_order();
            let group = self.browser.saved.groups.len();
            self.browser.saved.groups.push(Group {
                name: "New group".into(),
                color: group % 8,
            });
            self.browser.saved.tabs[target_index].group = Some(group);
            if let Some(entry) = order
                .iter_mut()
                .find(|entry| **entry == TabStripEntry::Tab(target))
            {
                *entry = TabStripEntry::Group(group);
            }
            self.browser.saved.tab_order = order;
            group
        };
        request.destination = CollectionId::new(format!("group:{group}"));
        request.slot = InsertionSlot(
            self.browser
                .saved
                .tabs
                .iter()
                .filter(|tab| tab.group == Some(group) && !tab.pinned)
                .position(|tab| tab.key == target)
                .expect("target in group")
                + 1,
        );
        self.apply_tab_drop(request);
    }
    fn tab_row(&self, i: usize, layout: Layout, plan: Option<&TopTabPlan>) -> Element<'_, Message> {
        let tab = &self.browser.saved.tabs[i];
        let selected = self.browser.library.is_none() && i == self.browser.saved.active;
        let icons = layout == Layout::Icons || plan.is_some_and(|p| p.icons);
        let rail = layout == Layout::Icons;
        let top = layout == Layout::Top;
        let height = self.chrome_metrics().tab - if top { 4.0 } else { 0.0 };
        // The row's whole height: in the rail the tile and the reach above it.
        let row_height = height + if rail { RAIL_REACH_Y } else { 0.0 };
        let close_on_hover = !(top && icons && plan.is_some_and(|p| p.width < 44.));
        let in_container = self
            .browser
            .saved
            .containers
            .iter()
            .any(|entry| Some(entry.id) == tab.container);
        let cell = enzui_widgets::hover_group::enz_hover_group_with(
            move |hovered| -> Element<'_, Message> {
                let document = self.browser.pdf_requests.contains_key(&tab.key)
                    || if i == self.browser.saved.active {
                        self.browser.pdf.is_some()
                    } else {
                        self.browser.sleeping[i]
                            .as_ref()
                            .is_some_and(|tab| tab.pdf.is_some())
                    };
                let symbol = if document {
                    Some("file-text")
                } else {
                    InternalPage::from_url(&tab.url).map(InternalPage::icon)
                };
                let mut favicon: Element<'_, Message> = if let Some(symbol) = symbol {
                    container(glyph(symbol, 16.)).width(16).height(16).into()
                } else if let Some(handle) = tab
                    .favicon
                    .clone()
                    .or_else(|| self.browser.favicons.get(&tab.url))
                {
                    iced::widget::image(handle).width(16).height(16).into()
                } else {
                    container(glyph("globe", 16.)).width(16).height(16).into()
                };
                let mut label = row![]
                    .spacing(if icons || top { 4 } else { 8 })
                    .align_y(iced::Alignment::Center);
                if let Some(entry) = self
                    .browser
                    .saved
                    .containers
                    .iter()
                    .find(|c| Some(c.id) == tab.container)
                {
                    if icons {
                        let color = group_ink(entry.color, self.look().palette.is_dark());
                        favicon = row![
                            container(Space::new()).width(3).height(18).style(move |_| {
                                container::Style {
                                    background: Some(color.into()),
                                    border: iced::Border {
                                        radius: 2.into(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }
                            }),
                            favicon
                        ]
                        .spacing(4)
                        .align_y(iced::Alignment::Center)
                        .into();
                    } else {
                        label = label.push(self.container_mark(entry, 13.));
                    }
                }
                // A tab that sounds says so at the corner of its icon, and
                // one the person silenced keeps saying that it is: a button
                // of its own, laid over the corner below.
                let sound = self.tab_sound(tab.key);
                // The colour the tab is painted in now, as the style below
                // has it; an unselected tab at rest shows the window's.
                let ground = {
                    let p = &self.look().palette;
                    if selected {
                        p.surface_selected
                    } else if hovered {
                        p.surface_hover.mix(p.text, 0.07)
                    } else if self.chrome_metrics().frame == crate::appearance::Frame::Solid {
                        p.background
                    } else {
                        // A grained or glass frame has no one colour.
                        iced::Color::TRANSPARENT
                    }
                };
                label = label.push(favicon);
                if !icons {
                    label = label.push(
                        text(tab.title.clone())
                            .size(13)
                            .wrapping(iced::widget::text::Wrapping::None)
                            .ellipsis(iced::widget::text::Ellipsis::End)
                            .width(Length::Fill),
                    );
                }
                let select: Element<'_, Message> =
                    enzui_widgets::focus_button::focus_button(label, c(Command::Select(i)))
                        .id(iced::advanced::widget::Id::from(format!("tab-{}", tab.key)))
                        .semantics(
                            enzui_core::semantics::Semantics::new(enzui_core::semantics::Role::Tab)
                                .label(tab.title.clone())
                                .selected(selected),
                        )
                        .width(Length::Fill)
                        .height(height)
                        .padding([
                            0,
                            if icons {
                                0
                            } else if top {
                                4
                            } else {
                                6
                            },
                        ])
                        .style_focus(|theme: &iced::Theme, _| iced::widget::button::Style {
                            background: None,
                            text_color: palette(theme).text,
                            ..Default::default()
                        })
                        .into();
                let close: Element<'_, Message> = if hovered && !tab.pinned && close_on_hover {
                    if icons {
                        enzui_widgets::focus_button::focus_button(
                            container(glyph("x", 8.)).center_x(12).center_y(12),
                            c(Command::Close(i)),
                        )
                        .width(12)
                        .height(12)
                        .padding(0)
                        .semantics(
                            enzui_core::semantics::Semantics::new(
                                enzui_core::semantics::Role::Button,
                            )
                            .label(format!("Close {}", tab.title)),
                        )
                        .style_focus(|theme, status| {
                            let p = palette(theme);
                            iced::widget::button::Style {
                                background: Some(
                                    if matches!(
                                        status,
                                        enzui_widgets::focus_button::FocusStatus::Hovered
                                            | enzui_widgets::focus_button::FocusStatus::Pressed
                                    ) {
                                        p.error
                                    } else {
                                        p.surface_elevated
                                    }
                                    .into(),
                                ),
                                text_color: if matches!(
                                    status,
                                    enzui_widgets::focus_button::FocusStatus::Hovered
                                        | enzui_widgets::focus_button::FocusStatus::Pressed
                                ) {
                                    iced::Color::WHITE
                                } else {
                                    p.text
                                },
                                border: iced::Border {
                                    radius: 6.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        })
                        .into()
                    } else if top {
                        enzui_widgets::focus_button::focus_button(
                            container(glyph("x", 8.)).center_x(12).center_y(12),
                            c(Command::Close(i)),
                        )
                        .id(iced::widget::Id::from(format!("tab-close-{}", tab.key)))
                        .semantics(
                            enzui_core::semantics::Semantics::new(
                                enzui_core::semantics::Role::Button,
                            )
                            .label(format!("Close {}", tab.title)),
                        )
                        .width(12)
                        .height(12)
                        .padding(0)
                        .style_focus(|theme, status| {
                            let p = palette(theme);
                            iced::widget::button::Style {
                                background: Some(
                                    if matches!(
                                        status,
                                        enzui_widgets::focus_button::FocusStatus::Hovered
                                            | enzui_widgets::focus_button::FocusStatus::Pressed
                                    ) {
                                        p.surface_hover
                                    } else {
                                        p.surface_elevated
                                    }
                                    .into(),
                                ),
                                text_color: p.text,
                                border: iced::Border {
                                    radius: 6.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        })
                        .into()
                    } else {
                        ink_icon(
                            "x",
                            &format!("Close {}", tab.title),
                            c(Command::Close(i)),
                            24.0,
                            12.0,
                        )
                    }
                } else {
                    Space::new().width(24).height(24).into()
                };
                if rail {
                    // The tile is painted here, inside the row; the row's own
                    // ground stays clear, and its buttons sit at its corners.
                    let tile = container(select)
                        .width(Length::Fill)
                        .height(height)
                        .style(move |theme| {
                            let p = palette(theme);
                            container::Style {
                                background: (selected || hovered).then(|| {
                                    if selected {
                                        p.surface_selected
                                    } else {
                                        p.surface_hover.mix(p.text, 0.07)
                                    }
                                    .into()
                                }),
                                border: iced::Border {
                                    radius: 7.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        });
                    let mut layers: Vec<Element<'_, Message>> = vec![
                        container(tile)
                            .padding(iced::Padding {
                                top: RAIL_REACH_Y,
                                right: RAIL_REACH_X,
                                bottom: 0.0,
                                left: RAIL_REACH_X,
                            })
                            .into(),
                        container(close)
                            .align_right(Length::Fill)
                            .align_top(Length::Fill)
                            .into(),
                    ];
                    if let Some(sound) = sound {
                        layers.push(
                            container(self.sound_button(i, sound, ground))
                                .align_left(Length::Fill)
                                .align_top(Length::Fill)
                                .into(),
                        );
                    }
                    return iced::widget::stack(layers)
                        .width(Length::Fill)
                        .height(row_height)
                        .into();
                }
                let body: Element<'_, Message> = if top || icons {
                    iced::widget::stack![
                        select,
                        container(close)
                            .align_right(Length::Fill)
                            .align_top(Length::Fill)
                            .padding(if top {
                                iced::Padding {
                                    top: 2.,
                                    right: 3.,
                                    bottom: 0.,
                                    left: 0.,
                                }
                            } else {
                                iced::Padding::ZERO
                            })
                    ]
                    .width(Length::Fill)
                    .height(height)
                    .into()
                } else {
                    row![select, close]
                        .align_y(iced::Alignment::Center)
                        .width(Length::Fill)
                        .height(height)
                        .into()
                };
                let Some(sound) = sound else {
                    return body;
                };
                // The button stands over the icon's top left corner, a
                // little outside it.
                let reach = sound::REACH + 3.0;
                if icons {
                    // The icon is in the middle of the row, after the 3 px
                    // container stripe and the gap after it when there is
                    // one: a box centred where the icon is, as much larger
                    // than it as the button reaches, holds the button at its
                    // top left.
                    let side = sound::MARK.max(16.0 + 2.0 * reach);
                    let shift = if in_container { 7.0 } else { 0.0 };
                    return iced::widget::stack![
                        body,
                        container(
                            container(self.sound_button(i, sound, ground))
                                .width(side)
                                .height(side)
                                .align_left(side)
                                .align_top(side)
                        )
                        .padding(iced::Padding {
                            left: shift,
                            ..iced::Padding::ZERO
                        })
                        .center(Length::Fill)
                    ]
                    .into();
                }
                // Where the icon is in the row: after the row's padding and,
                // in a container, its 13 px mark and the gap after it.
                let (inset, gap) = if top { (4.0, 4.0) } else { (6.0, 8.0) };
                let icon_x = inset + if in_container { 13.0 + gap } else { 0.0 };
                let icon_y = (height - 16.0) / 2.0;
                iced::widget::stack![
                    body,
                    container(self.sound_button(i, sound, ground))
                        .padding(iced::Padding {
                            top: (icon_y - reach).max(0.0),
                            left: (icon_x - reach).max(0.0),
                            ..iced::Padding::ZERO
                        })
                        .width(Length::Fill)
                        .height(Length::Fill)
                ]
                .into()
            },
        )
        .width(if top {
            Length::Fixed(plan.map_or(196., |p| p.width))
        } else {
            Length::Fill
        })
        .height(row_height)
        .style(move |theme, hovered| {
            let p = palette(theme);
            enzui_widgets::hover_group::HoverGroupStyle {
                // The rail's row paints its tile itself, inside its reach.
                background: if rail {
                    iced::Color::TRANSPARENT
                } else if selected {
                    p.surface_selected
                } else if hovered {
                    p.surface_hover.mix(p.text, 0.07)
                } else {
                    iced::Color::TRANSPARENT
                },
                border_color: iced::Color::TRANSPARENT,
                border_width: 0.0,
                radius: 7.0,
            }
        })
        .into_element();
        let cell = enzui_widgets::tooltip::FadeTooltip::new(
            cell,
            self.browser
                .saved
                .containers
                .iter()
                .find(|entry| Some(entry.id) == tab.container)
                .map_or_else(
                    || tab.title.clone(),
                    |entry| format!("{} · {}", entry.name, tab.title),
                ),
            if top {
                iced::widget::tooltip::Position::Bottom
            } else if self.browser.saved.right {
                iced::widget::tooltip::Position::Left
            } else {
                iced::widget::tooltip::Position::Right
            },
        )
        .delay(std::time::Duration::from_millis(600));
        let cell: Element<'_, Message> =
            enzui_widgets::context_target::enz_context_target(cell, move |point| {
                c(Command::TabContext(i, point))
            })
            .width(if top {
                Length::Fixed(plan.map_or(196., |p| p.width))
            } else {
                Length::Fill
            })
            .height(row_height)
            .into_element();
        iced::widget::mouse_area(cell)
            .on_middle_press(c(Command::Close(i)))
            .into()
    }
    pub(super) fn visible_tab_drop(
        &mut self,
        mut request: enzui_core::drag::DropRequest,
        keys: &[u64],
    ) {
        if request.slot.0 != usize::MAX {
            let order: Vec<_> = if request.destination.0 == "main" {
                self.browser
                    .strip_order()
                    .iter()
                    .map(|e| match e {
                        TabStripEntry::Tab(k) => (*k, keys.contains(k)),
                        TabStripEntry::Group(g) => (
                            0,
                            self.browser
                                .saved
                                .tabs
                                .iter()
                                .any(|t| t.group == Some(*g) && keys.contains(&t.key)),
                        ),
                    })
                    .collect()
            } else {
                self.browser
                    .saved
                    .tabs
                    .iter()
                    .filter(|t| tab_collection(t) == request.destination.0)
                    .map(|t| (t.key, keys.contains(&t.key)))
                    .collect()
            };
            let shown: Vec<usize> = order
                .iter()
                .enumerate()
                .filter(|(_, (_, shown))| *shown)
                .map(|(i, _)| i)
                .collect();
            request.slot.0 = shown
                .get(request.slot.0)
                .copied()
                .unwrap_or_else(|| shown.last().map_or(order.len(), |i| i + 1));
        }
        self.apply_tab_drop(request);
    }
    pub(super) fn apply_tab_drop(&mut self, request: enzui_core::drag::DropRequest) {
        use enzui_core::drag::DragOperation;
        if request.operation != DragOperation::Move || request.items.is_empty() {
            return;
        }
        self.browser.normalize_tab_order();
        let source = request.source.0.as_str();
        let dest = request.destination.0.as_str();
        let mut entries = Vec::new();
        let mut keys = Vec::new();
        for id in &request.items {
            if let Some(key) =
                id.0.strip_prefix("tab:")
                    .and_then(|v| v.parse::<u64>().ok())
            {
                if !self
                    .browser
                    .saved
                    .tabs
                    .iter()
                    .any(|t| t.key == key && tab_collection(t) == source)
                    || keys.contains(&key)
                {
                    return;
                }
                keys.push(key);
                entries.push(TabStripEntry::Tab(key));
            } else if let Some(g) =
                id.0.strip_prefix("group:")
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|g| *g < self.browser.saved.groups.len())
            {
                if source != "main" || dest != "main" || entries.contains(&TabStripEntry::Group(g))
                {
                    return;
                }
                entries.push(TabStripEntry::Group(g));
            } else {
                return;
            }
        }
        if dest == "main" {
            let mut order = self.browser.strip_order();
            let slot = request.slot.0.min(order.len());
            let before = order
                .iter()
                .take(slot)
                .filter(|e| entries.contains(e))
                .count();
            order.retain(|e| !entries.contains(e));
            for tab in &mut self.browser.saved.tabs {
                if keys.contains(&tab.key) {
                    tab.group = None;
                    tab.pinned = false;
                }
            }
            order.splice(
                slot.saturating_sub(before)..slot.saturating_sub(before),
                entries,
            );
            self.browser.saved.tab_order = order;
        } else {
            let (pinned, group) = if dest == "pinned" {
                (true, None)
            } else if let Some(g) = dest
                .strip_prefix("group:")
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|g| *g < self.browser.saved.groups.len())
            {
                (false, Some(g))
            } else {
                return;
            };
            let targets: Vec<_> = self
                .browser
                .saved
                .tabs
                .iter()
                .filter(|t| tab_collection(t) == dest)
                .map(|t| t.key)
                .collect();
            let slot = request.slot.0.min(targets.len());
            let before = targets
                .iter()
                .take(slot)
                .filter(|k| keys.contains(k))
                .count();
            let remaining: Vec<_> = targets.into_iter().filter(|k| !keys.contains(k)).collect();
            let active = self.browser.saved.tabs[self.browser.saved.active].key;
            let mut moving = Vec::new();
            let mut rest = Vec::new();
            for (mut tab, host) in std::mem::take(&mut self.browser.saved.tabs)
                .into_iter()
                .zip(std::mem::take(&mut self.browser.sleeping))
            {
                if keys.contains(&tab.key) {
                    tab.pinned = pinned;
                    tab.group = group;
                    moving.push((tab, host));
                } else {
                    rest.push((tab, host));
                }
            }
            let at = slot.saturating_sub(before);
            let insert = remaining
                .get(at)
                .and_then(|k| rest.iter().position(|(t, _)| t.key == *k))
                .or_else(|| {
                    remaining
                        .last()
                        .and_then(|k| rest.iter().position(|(t, _)| t.key == *k).map(|i| i + 1))
                })
                .unwrap_or(rest.len());
            rest.splice(insert..insert, moving);
            let (tabs, hosts) = rest.into_iter().unzip();
            self.browser.saved.tabs = tabs;
            self.browser.sleeping = hosts;
            self.browser.saved.active = self
                .browser
                .saved
                .tabs
                .iter()
                .position(|t| t.key == active)
                .unwrap_or(0);
            self.browser
                .saved
                .tab_order
                .retain(|e| !matches!(e,TabStripEntry::Tab(key) if keys.contains(key)));
        }
        self.browser.commit_tab_order();
    }
}
pub(super) fn tab_collection(tab: &TabInfo) -> String {
    if tab.pinned {
        "pinned".into()
    } else {
        tab.group
            .map_or_else(|| "main".into(), |g| format!("group:{g}"))
    }
}
impl Browser {
    pub(super) fn strip_order(&self) -> Vec<TabStripEntry> {
        let mut valid = Vec::new();
        for tab in &self.saved.tabs {
            if !tab.pinned {
                let entry = tab
                    .group
                    .map_or(TabStripEntry::Tab(tab.key), TabStripEntry::Group);
                if !valid.contains(&entry) {
                    valid.push(entry);
                }
            }
        }
        for g in 0..self.saved.groups.len() {
            let entry = TabStripEntry::Group(g);
            if !valid.contains(&entry) {
                valid.push(entry);
            }
        }
        let mut order = Vec::new();
        for entry in self.saved.tab_order.iter().chain(valid.iter()) {
            if valid.contains(entry) && !order.contains(entry) {
                order.push(entry.clone());
            }
        }
        order
    }
    /// Ends group `g`. Its tabs stay where it stood in the strip, in no
    /// group, and every later group moves down one place, in the tabs and
    /// in the strip's order alike.
    pub(super) fn remove_group(&mut self, g: usize) {
        if g >= self.saved.groups.len() {
            return;
        }
        let mut order = Vec::new();
        for entry in self.strip_order() {
            match entry {
                TabStripEntry::Group(x) if x == g => order.extend(
                    self.saved
                        .tabs
                        .iter()
                        .filter(|t| !t.pinned && t.group == Some(g))
                        .map(|t| TabStripEntry::Tab(t.key)),
                ),
                TabStripEntry::Group(x) if x > g => order.push(TabStripEntry::Group(x - 1)),
                other => order.push(other),
            }
        }
        for tab in &mut self.saved.tabs {
            tab.group = match tab.group {
                Some(x) if x == g => None,
                Some(x) if x > g => Some(x - 1),
                other => other,
            };
        }
        self.saved.groups.remove(g);
        self.saved.tab_order = order;
        self.saved.tab_order = self.strip_order();
    }
    /// The tab `key` was just opened: its row makes an entrance. Entries
    /// older than any entrance are let go here.
    pub(crate) fn mark_tab_entrance(&mut self, key: u64) {
        let now = std::time::Instant::now();
        self.tab_entrances
            .retain(|_, at| now.duration_since(*at) < TAB_ENTRANCE * 4);
        self.tab_entrances.insert(key, now);
    }
    pub(super) fn normalize_tab_order(&mut self) {
        for t in &mut self.saved.tabs {
            if t.pinned || t.group.is_some_and(|g| g >= self.saved.groups.len()) {
                t.group = None;
            }
        }
        self.saved.tab_order = self.strip_order();
    }
    pub(crate) fn replace_order_key(&mut self, old: u64, new: u64) {
        // A tab takes a new key with each document it is sent to; what is
        // kept by its key about the tab itself goes with it.
        if self.audible.remove(&old) {
            self.audible.insert(new);
        }
        if self.muted.remove(&old) {
            self.muted.insert(new);
        }
        if self.prompt_tab == Some(old) {
            self.prompt_tab = Some(new);
        }
        if self.fullscreen.page == Some(old) {
            self.fullscreen.page = Some(new);
        }
        // A window a page opened stays that page's window, on either side.
        if let Some(opener) = self.popup_openers.remove(&old) {
            self.popup_openers.insert(new, opener);
        }
        for opener in self.popup_openers.values_mut() {
            if *opener == old {
                *opener = new;
            }
        }
        for entry in &mut self.saved.tab_order {
            if *entry == TabStripEntry::Tab(old) {
                *entry = TabStripEntry::Tab(new);
            }
        }
    }
    pub(crate) fn replace_tab_key(&mut self, index: usize, key: u64) {
        let old = self.saved.tabs[index].key;
        self.saved.tabs[index].key = key;
        crate::page_ids::rename(old, key);
        self.replace_order_key(old, key);
    }
    pub(super) fn commit_tab_order(&mut self) {
        self.normalize_tab_order();
        let active = self.saved.tabs[self.saved.active].key;
        let mut keys: Vec<_> = self
            .saved
            .tabs
            .iter()
            .filter(|t| t.pinned)
            .map(|t| t.key)
            .collect();
        for entry in &self.saved.tab_order {
            match entry {
                TabStripEntry::Tab(k) => keys.push(*k),
                TabStripEntry::Group(g) => keys.extend(
                    self.saved
                        .tabs
                        .iter()
                        .filter(|t| !t.pinned && t.group == Some(*g))
                        .map(|t| t.key),
                ),
            }
        }
        let mut pool: std::collections::BTreeMap<_, _> = std::mem::take(&mut self.saved.tabs)
            .into_iter()
            .zip(std::mem::take(&mut self.sleeping))
            .map(|(t, h)| (t.key, (t, h)))
            .collect();
        let mut tabs = Vec::new();
        let mut hosts = Vec::new();
        for key in keys {
            if let Some((tab, host)) = pool.remove(&key) {
                tabs.push(tab);
                hosts.push(host);
            }
        }
        for (_, (tab, host)) in pool {
            tabs.push(tab);
            hosts.push(host);
        }
        self.saved.tabs = tabs;
        self.sleeping = hosts;
        self.saved.active = self
            .saved
            .tabs
            .iter()
            .position(|t| t.key == active)
            .unwrap_or(0);
    }
}

#[cfg(test)]
mod adaptive_top_tests {
    use super::*;
    fn browser(count: usize, groups: usize) -> Browser {
        let mut browser = Browser::load();
        browser.saved = Saved::default();
        browser.viewport = iced::Size::new(1400., 900.);
        browser.saved.groups = (0..groups)
            .map(|g| Group {
                name: format!("Group {g}"),
                color: g % 8,
            })
            .collect();
        browser.saved.tabs = (0..count)
            .map(|i| TabInfo {
                key: i as u64 + 1,
                favicon: None,
                page_copy: None,
                url: format!("https://example.com/{i}"),
                title: format!("A useful long page title {i}"),
                pinned: false,
                group: if groups > 0 { Some(i % groups) } else { None },
                container: None,
            })
            .collect();
        browser
    }
    fn density_app(count: usize, groups: usize) -> App {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser = browser(count, groups);
        app.browser.saved.layout = Layout::Top;
        app.browser.viewport = iced::Size::new(1280., 800.);
        app.browser.private = true;
        app.browser.sleeping = (0..count).map(|_| None).collect();
        app
    }
    #[test]
    fn tab_density_middle_click_empty_top_strip_opens_new_but_tab_middle_click_closes() {
        let app = density_app(5, 0);
        for (point, new) in [
            (iced::Point::new(1120., 14.), true),
            (iced::Point::new(90., 14.), false),
        ] {
            let mut ui = iced_test::Simulator::with_size(
                Default::default(),
                iced::Size::new(1260., 30.),
                app.tab_strip_for(Layout::Top),
            );
            ui.point_at(point);
            ui.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Middle,
            ))]);
            let messages = ui.into_messages().collect::<Vec<_>>();
            assert_eq!(messages.len(), 1, "middle click must route once");
            assert!(
                matches!(&messages[0], Message::Chrome(Command::New)) == new,
                "empty strip opens New"
            );
            if !new {
                assert!(
                    matches!(&messages[0], Message::Chrome(Command::Close(0))),
                    "tab middle click closes without opening New"
                );
            }
        }
    }
    #[test]
    fn tab_density_icon_groups_keep_normal_tab_width_inside_wider_backgrounds() {
        for (theme_name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            let mut app = density_app(5, 1);
            app.browser.saved.tabs[0].group = None;
            app.theme =
                enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(
                    theme,
                )
                .0;
            let strip = container(app.tab_strip_for(Layout::Icons))
                .width(48)
                .height(700)
                .style(|theme| container::Style {
                    background: Some(palette(theme).background.into()),
                    ..Default::default()
                });
            let mut ui = iced_test::Simulator::with_size(
                Default::default(),
                iced::Size::new(48., 700.),
                strip,
            );
            let plain = ui
                .find(iced::widget::Id::new("tab-1"))
                .expect("ordinary icon tab")
                .bounds();
            let grouped = ui
                .find(iced::widget::Id::new("tab-2"))
                .expect("grouped icon tab")
                .bounds();
            let group = ui
                .find(iced::widget::Id::new("tab-group-0"))
                .expect("group background")
                .bounds();
            assert_eq!(plain.width, 40., "ordinary icon tabs retain their width");
            assert_eq!(
                grouped.width, plain.width,
                "group padding must not squeeze the tab"
            );
            assert_eq!(
                group.width, 44.,
                "group color extends two pixels beyond each tab edge"
            );
            if let Some(output) = std::env::var_os("ENZIM_TAB_VISUAL_OUTPUT") {
                let path = std::path::PathBuf::from(output).join(format!("icons-{theme_name}"));
                assert!(
                    ui.snapshot(&app.theme.iced_theme())
                        .expect("icon rail render")
                        .matches_image(path)
                        .expect("PNG capture")
                );
            }
        }
    }
    #[test]
    fn tab_density_twenty_tabs_keep_text_before_icon_only_mode() {
        let app = density_app(20, 2);
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(1260., 30.),
            app.tab_strip_for(Layout::Top),
        );
        assert!(
            ui.find("A useful long page title 0")
                .expect("title remains at twenty tabs")
                .bounds()
                .width
                >= 8.
        );
        let app = density_app(50, 0);
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(1260., 30.),
            app.tab_strip_for(Layout::Top),
        );
        assert!(
            ui.find(iced::widget::Id::new("tab-1")).is_ok(),
            "icon-only strip still contains the tab"
        );
        assert!(
            ui.find("A useful long page title 0").is_err(),
            "titles yield when there is no room for a character"
        );
    }
    #[test]
    fn tab_density_titles_use_close_slot_and_hover_does_not_resize_them() {
        for (theme_name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            for count in [5, 10, 15, 20] {
                for groups in [0, 2] {
                    let mut app = density_app(count, groups);
                    app.theme = enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(theme.clone()).0;
                    let strip = container(app.tab_strip_for(Layout::Top))
                        .padding([8, 10])
                        .width(1280)
                        .height(46)
                        .style(|theme| container::Style {
                            background: Some(palette(theme).background.into()),
                            ..Default::default()
                        });
                    let mut ui = iced_test::Simulator::with_size(
                        Default::default(),
                        iced::Size::new(1280., 46.),
                        strip,
                    );
                    let tab = ui
                        .find(iced::widget::Id::new("tab-1"))
                        .expect("first tab")
                        .bounds();
                    let title = ui
                        .find("A useful long page title 0")
                        .expect("first title")
                        .bounds();
                    assert!(
                        title.width >= tab.width - 30.,
                        "title must use the close slot: tab={tab:?}, title={title:?}"
                    );
                    for hovered in [false, true] {
                        if hovered {
                            ui.point_at(tab.center());
                            ui.draw(&app.theme.iced_theme());
                        }
                        if let Some(output) = std::env::var_os("ENZIM_TAB_VISUAL_OUTPUT") {
                            let path = std::path::PathBuf::from(output).join(format!(
                                "tabs-{theme_name}-{count}-groups{groups}-hover{hovered}"
                            ));
                            assert!(
                                ui.snapshot(&app.theme.iced_theme())
                                    .expect("tab strip render")
                                    .matches_image(path)
                                    .expect("PNG capture")
                            );
                        }
                    }
                    let after = ui
                        .find("A useful long page title 0")
                        .expect("hovered title")
                        .bounds();
                    assert_eq!(title.width, after.width, "hover must not reflow the title");
                    let close = ui
                        .find(iced::widget::Id::new("tab-close-1"))
                        .expect("hover exposes close button");
                    ui.click(iced::widget::Id::new("tab-close-1"))
                        .expect("close button is reachable");
                    assert!(
                        ui.into_messages()
                            .any(|message| matches!(message, Message::Chrome(Command::Close(0)))),
                        "hover close routes to the tab"
                    );
                    assert_eq!(close.bounds().width, 12.);
                    assert!(
                        (tab.x + tab.width - close.bounds().x - close.bounds().width - 3.).abs()
                            < 0.1,
                        "close keeps a right inset"
                    );
                    assert!(
                        (close.bounds().y - tab.y - 2.).abs() < 0.1,
                        "close sits at the upper edge with an inset"
                    );
                }
            }
        }
    }
    #[test]
    fn tab_review_center_drop_groups_tabs_and_edge_drop_reorders_without_grouping() {
        use enzui_core::drag::{CollectionId, DragOperation, DropRequest, InsertionSlot, ItemId};
        let request = |source: &str, key, slot| DropRequest {
            source: CollectionId::new(source),
            destination: CollectionId::new("main"),
            items: vec![ItemId::new(format!("tab:{key}"))],
            slot: InsertionSlot(slot),
            operation: DragOperation::Move,
        };
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser = browser(4, 0);
        app.browser.private = true;
        app.browser.sleeping = (0..4).map(|_| None).collect();
        app.browser.saved.active = 2;
        let active = app.browser.saved.tabs[2].key;
        app.group_tab_drop(request("missing", 1, 0), 2);
        assert!(
            app.browser.saved.groups.is_empty(),
            "invalid source cannot create an empty group"
        );
        app.group_tab_drop(request("main", 2, 0), 2);
        assert!(
            app.browser.saved.groups.is_empty(),
            "self-drop does not group"
        );
        app.group_tab_drop(request("main", 1, 0), 2);
        assert_eq!(app.browser.saved.groups.len(), 1);
        assert_eq!(
            app.browser
                .saved
                .tabs
                .iter()
                .filter(|tab| tab.group == Some(0))
                .map(|tab| tab.key)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(app.browser.saved.tabs[app.browser.saved.active].key, active);
        app.group_tab_drop(request("main", 4, 0), 2);
        assert_eq!(
            app.browser.saved.groups.len(),
            1,
            "dropping onto a member joins its group"
        );
        assert_eq!(
            app.browser
                .saved
                .tabs
                .iter()
                .filter(|tab| tab.group == Some(0))
                .map(|tab| tab.key)
                .collect::<Vec<_>>(),
            vec![2, 4, 1]
        );
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser = browser(4, 0);
        app.browser.private = true;
        app.browser.sleeping = (0..4).map(|_| None).collect();
        app.apply_tab_drop(request("main", 1, 3));
        assert!(app.browser.saved.groups.is_empty());
        assert_eq!(
            app.browser
                .saved
                .tabs
                .iter()
                .map(|tab| tab.key)
                .collect::<Vec<_>>(),
            vec![2, 3, 1, 4]
        );
        for horizontal in [false, true] {
            let size = iced::Size::new(160., 32.);
            let target = tab_group_drop_item(1, size, horizontal);
            assert!(
                target.accepts_drop,
                "tab centers must accept grouping drops"
            );
            assert!(!target.drag_source, "reorder list owns the drag source");
            let bounds = target.bounds;
            assert!(bounds.contains(iced::Point::new(80., 16.)));
            for point in if horizontal {
                [iced::Point::new(2., 16.), iced::Point::new(158., 16.)]
            } else {
                [iced::Point::new(80., 2.), iced::Point::new(80., 30.)]
            } {
                assert!(
                    !bounds.contains(point),
                    "tab edges must remain available for insertion"
                );
            }
        }
    }
    #[test]
    fn adaptive_top_drops_map_visible_gaps_without_reordering_hidden_tabs() {
        use enzui_core::drag::{CollectionId, DragOperation, DropRequest, InsertionSlot, ItemId};
        for (slot, expected) in [
            (0, vec![1, 2, 3, 10, 4, 5, 6, 7, 8, 9]),
            (3, vec![1, 2, 3, 4, 5, 6, 10, 7, 8, 9]),
        ] {
            let mut a = App::new(enzui_theme::EnzTheme::dark());
            a.browser = browser(10, 0);
            a.browser.private = true;
            a.browser.sleeping = (0..10).map(|_| None).collect();
            a.visible_tab_drop(
                DropRequest {
                    source: CollectionId::new("main"),
                    destination: CollectionId::new("main"),
                    items: vec![ItemId::new("tab:10")],
                    slot: InsertionSlot(slot),
                    operation: DragOperation::Move,
                },
                &[4, 5, 6],
            );
            assert_eq!(
                a.browser
                    .saved
                    .tabs
                    .iter()
                    .map(|t| t.key)
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    #[test]
    fn adaptive_top_tabs_truncate_before_icons_and_bound_overflow_around_selection() {
        let few = TopTabPlan::new(&browser(3, 0));
        let many = TopTabPlan::new(&browser(12, 0));
        let icons = TopTabPlan::new(&browser(40, 0));
        assert_eq!(few.width, 196.);
        assert!(many.width < few.width && !many.icons && !many.overflow);
        assert!(icons.icons && !icons.overflow);
        let mut crowded = browser(120, 10);
        crowded.saved.active = 119;
        let plan = TopTabPlan::new(&crowded);
        assert!(plan.overflow && plan.icons);
        assert!(plan.contains(120));
        assert!(plan.visible.len() < 120);
        let footprint = plan.visible.len() as f32 * (plan.width + 2.)
            + crowded
                .saved
                .tabs
                .iter()
                .filter(|t| plan.contains(t.key))
                .filter_map(|t| t.group)
                .collect::<std::collections::BTreeSet<_>>()
                .len() as f32
                * (plan.header + 8.)
            + 32.;
        assert!(footprint <= crowded.viewport.width - 54.);
        let crowded = browser(120, 120);
        let plan = TopTabPlan::new(&crowded);
        assert!(plan.visible.len() >= 15);
        assert!(plan.contains(crowded.saved.tabs[crowded.saved.active].key));
    }
    #[test]
    fn browser_tests_empty_sidebar_drop_moves_a_group_member_to_the_end() {
        let mut app = super::super::browser_tests::app();
        app.browser_mode = true;
        app.new_tab("https://example.org/one".into());
        let moving = app.browser.saved.tabs[1].key;
        let _ = app.browser_command(Command::Group(1));
        app.new_tab("https://example.org/two".into());
        let message = drop_message(
            enzui_core::drag::DropRequest {
                source: "group:0".into(),
                destination: "main".into(),
                items: vec![enzui_core::drag::ItemId::new(format!("tab:{moving}"))],
                slot: enzui_core::drag::InsertionSlot(usize::MAX),
                operation: enzui_core::drag::DragOperation::Move,
            },
            None,
        );
        let _ = app.update(message);
        let tab = app
            .browser
            .saved
            .tabs
            .iter()
            .find(|tab| tab.key == moving)
            .unwrap();
        assert_eq!(tab.group, None);
        assert!(!tab.pinned);
        assert_eq!(
            app.browser.strip_order().last(),
            Some(&TabStripEntry::Tab(moving))
        );
    }
}
