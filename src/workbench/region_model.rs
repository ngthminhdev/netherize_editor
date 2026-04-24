/// ID chuẩn hóa cho các vùng chính của Workbench.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegionId {
    Root,
    TopBar,
    LeftSidebar,
    Center,
    RightSidebar,
    BottomPanel,
    StatusBar,
    OverlayLayer,
}

impl RegionId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Root => "Root",
            Self::TopBar => "TopBar",
            Self::LeftSidebar => "LeftSidebar",
            Self::Center => "Center(Editor)",
            Self::RightSidebar => "RightSidebar",
            Self::BottomPanel => "BottomPanel",
            Self::StatusBar => "StatusBar",
            Self::OverlayLayer => "OverlayLayer",
        }
    }
}

/// Bounding box theo pixel screen-space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RegionBounds {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }
}

/// Node trong region tree.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionNode {
    pub id: RegionId,
    pub bounds: RegionBounds,
    pub visible: bool,
    pub children: Vec<RegionNode>,
}

impl RegionNode {
    pub fn new(id: RegionId, bounds: RegionBounds, visible: bool) -> Self {
        Self {
            id,
            bounds,
            visible,
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<RegionNode>) -> Self {
        self.children = children;
        self
    }
}

/// View phẳng để render: layout engine quản lý bounds/ownership,
/// còn renderer chỉ cần đọc danh sách region đã tính sẵn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatRegion {
    pub id: RegionId,
    pub bounds: RegionBounds,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionModel {
    pub root: RegionNode,
}

impl RegionModel {
    pub fn new(root_bounds: RegionBounds) -> Self {
        Self {
            root: RegionNode::new(RegionId::Root, root_bounds, true),
        }
    }

    pub fn with_children(mut self, children: Vec<RegionNode>) -> Self {
        self.root.children = children;
        self
    }

    pub fn flatten(&self) -> Vec<FlatRegion> {
        let mut out = Vec::new();
        Self::flatten_node(&self.root, &mut out);
        out
    }

    fn flatten_node(node: &RegionNode, out: &mut Vec<FlatRegion>) {
        out.push(FlatRegion {
            id: node.id,
            bounds: node.bounds,
            visible: node.visible,
        });
        for child in &node.children {
            Self::flatten_node(child, out);
        }
    }

    pub fn find(&self, id: RegionId) -> Option<RegionBounds> {
        Self::find_node(&self.root, id).map(|node| node.bounds)
    }

    fn find_node(node: &RegionNode, id: RegionId) -> Option<&RegionNode> {
        if node.id == id {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = Self::find_node(child, id) {
                return Some(found);
            }
        }
        None
    }
}
