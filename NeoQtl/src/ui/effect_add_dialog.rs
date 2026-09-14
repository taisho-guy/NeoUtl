use crate::ui::types::CatalogRow;

pub trait EffectCatalogSource {
    fn categories(&self) -> &[String];
    fn filtered(&self, query: &str, sort_mode: i32, category: &str) -> Vec<CatalogRow>;
}

pub struct EffectAddDialog {
    pub open: bool,
    pub query: String,
    pub sort_mode: i32,
    pub category_filter: String,
}

impl EffectAddDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            sort_mode: 0,
            category_filter: String::new(),
        }
    }

    pub fn open(&mut self) {
        self.query.clear();
        self.sort_mode = 0;
        self.category_filter.clear();
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn filtered_rows(&self, source: &dyn EffectCatalogSource) -> Vec<CatalogRow> {
        source.filtered(&self.query, self.sort_mode, &self.category_filter)
    }
}
