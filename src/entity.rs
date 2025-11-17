use uuid::Uuid;

pub type EntityId = Uuid;

#[derive(Debug)]
pub struct Entity {
    id: EntityId,
    transform: glam::Mat4,
    label: Option<String>,
}

impl Entity {
    pub fn new_id() -> EntityId {
        Uuid::new_v4()
    }

    pub fn new(transform: glam::Mat4, label: Option<String>) -> Self {
        Self {
            id: Self::new_id(),
            transform,
            label,
        }
    }

    pub fn translate(&mut self, translation: glam::Vec3) {        
        self.transform = glam::Mat4::from_translation(translation) * self.transform;
    }

    pub fn id(&self) -> EntityId {
        self.id
    }

    pub fn label(&self) -> &Option<String> {
        &self.label
    }

    pub fn transform(&self) -> glam::Mat4 {
        self.transform
    }

    pub fn set_transform(&mut self, transform: glam::Mat4) {
        self.transform = transform;
    }
}
