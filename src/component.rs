use std::{collections::HashMap, marker::PhantomData};

use bytemuck::{Pod, Zeroable};

use crate::{context::RenderContext, entity::EntityId};

pub struct ComponentId<T>(u32, PhantomData<T>);

impl<T> ComponentId<T> {
    pub fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }

    pub fn index(&self) -> u32 {
        self.0
    }
}

pub struct RelationStore<A, B> {
    mapping: Vec<u32>,
    capacity: usize,
    is_dirty: bool,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    _phantom: PhantomData<(A, B)>,
}

impl<A, B> RelationStore<A, B> {
    pub fn new(capacity: usize, visibility: wgpu::ShaderStages, context: &RenderContext) -> Self {
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Light bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let buffer = create_buffer::<u32>(capacity, context);
        let bind_group = create_bind_group(&buffer, &layout, context);

        Self {
            mapping: Vec::new(),
            capacity: capacity.max(1),
            is_dirty: false,
            buffer,
            layout,
            bind_group,
            _phantom: PhantomData,
        }
    }

    pub fn get_mapping(&self, index: usize) -> Option<u32> {
        self.mapping.get(index).copied()
    }

    pub fn link(&mut self, a: ComponentId<A>, b: ComponentId<B>, context: &RenderContext) {
        let index = a.index() as usize;
        if index >= self.mapping.len() {
            self.mapping.resize(index + 1, 0);
        }

        self.mapping[index] = b.index();

        if self.mapping.len() >= self.capacity {
            self.grow(context);
        }

        self.write(index, context);
    }

    pub fn is_dirty(&mut self) -> bool {
        let dirty = self.is_dirty;
        self.is_dirty = false;
        dirty
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    fn write(&self, index: usize, context: &RenderContext) {
        let offset = (index * std::mem::size_of::<u32>()) as u64;
        context
            .queue
            .write_buffer(&self.buffer, offset, bytemuck::bytes_of(&self.mapping[index]));
    }

    fn sync(&self, context: &RenderContext) {
        context
            .queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.mapping));
    }

    fn grow(&mut self, context: &RenderContext) {
        self.capacity *= 2;
        self.buffer = create_buffer::<u32>(self.capacity, context);
        self.bind_group = create_bind_group(&self.buffer, &self.layout, context);
        self.sync(context);
        self.is_dirty = true;
    }
}

pub struct ComponentStore<T: Pod + Zeroable + Copy> {
    components: Vec<T>,
    capacity: usize,
    index_map: HashMap<EntityId, usize>,
    free_indices: Vec<usize>,
    is_dirty: bool,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
}

impl<T: Pod + Zeroable + Copy> ComponentStore<T> {
    pub fn new(capacity: usize, context: &RenderContext) -> Self {
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Component bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let buffer = create_buffer::<T>(capacity, context);
        let bind_group = create_bind_group(&buffer, &layout, context);

        Self {
            components: Vec::new(),
            capacity: capacity.max(1),
            index_map: HashMap::new(),
            free_indices: Vec::new(),
            is_dirty: false,
            buffer,
            bind_group,
            layout,
        }
    }

    pub fn add(&mut self, entity: EntityId, component: T, context: &RenderContext) -> ComponentId<T> {
        if let Some(&index) = self.index_map.get(&entity) {
            self.components[index] = component;
            self.write(index, context);
            return ComponentId::new(index);
        }

        let index = if let Some(free) = self.free_indices.pop() {
            self.components[free] = component;
            free
        } else {
            let index = self.components.len();
            if self.components.len() >= self.capacity {
                self.grow(context);
            }

            self.components.push(component);
            index
        };

        self.index_map.insert(entity, index);
        self.write(index, context);
        ComponentId::new(index)
    }

    pub fn remove(&mut self, entity: &EntityId) {
        if let Some(index) = self.index_map.remove(entity) {
            self.free_indices.push(index);
        }
    }

    pub fn get(&self, entity: &EntityId) -> Option<&T> {
        self.index_map.get(entity).map(|&index| &self.components[index])
    }

    pub fn get_mut(&mut self, entity: &EntityId) -> Option<&mut T> {
        self.index_map.get(entity).map(|&index| &mut self.components[index])
    }

    pub fn get_by_id(&self, id: ComponentId<T>) -> Option<&T> {
        self.components.get(id.index() as usize)
    }

    pub fn get_by_index(&self, index: usize) -> Option<&T> {
        self.components.get(index)
    }

    pub fn iter_with_index(&self) -> impl Iterator<Item = (&EntityId, usize, &T)> {
        self.index_map
            .iter()
            .map(|(entity, &index)| (entity, index, &self.components[index]))
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn components(&self) -> &Vec<T> {
        &self.components
    }

    pub fn is_dirty(&mut self) -> bool {
        let dirty = self.is_dirty;
        self.is_dirty = false;
        dirty
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    fn write(&self, index: usize, context: &RenderContext) {
        let offset = (index * std::mem::size_of::<T>()) as u64;
        context
            .queue
            .write_buffer(&self.buffer, offset, bytemuck::bytes_of(&self.components[index]));
    }

    fn sync(&self, context: &RenderContext) {
        context
            .queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.components));
    }

    fn grow(&mut self, context: &RenderContext) {
        self.capacity *= 2;
        self.buffer = create_buffer::<T>(self.capacity, context);
        self.bind_group = create_bind_group(&self.buffer, &self.layout, context);
        self.sync(context);
        self.is_dirty = true;
    }
}

fn create_buffer<T>(capacity: usize, context: &RenderContext) -> wgpu::Buffer {
    context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Light storage buffer"),
        size: (capacity * std::mem::size_of::<T>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_bind_group(
    buffer: &wgpu::Buffer,
    layout: &wgpu::BindGroupLayout,
    context: &RenderContext,
) -> wgpu::BindGroup {
    context.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Light bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

pub struct LocalComponentStore<T> {
    components: Vec<T>,
    index_map: HashMap<EntityId, usize>,
    free_indices: Vec<usize>,
}

impl<T> LocalComponentStore<T> {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            index_map: HashMap::new(),
            free_indices: Vec::new(),
        }
    }

    pub fn add(&mut self, entity: EntityId, component: T) -> ComponentId<T> {
        if let Some(&index) = self.index_map.get(&entity) {
            self.components[index] = component;
            return ComponentId::new(index);
        }

        let index = if let Some(free) = self.free_indices.pop() {
            self.components[free] = component;
            free
        } else {
            let index = self.components.len();
            self.components.push(component);
            index
        };

        self.index_map.insert(entity, index);
        ComponentId::new(index)
    }

    pub fn remove(&mut self, entity: &EntityId) {
        if let Some(index) = self.index_map.remove(entity) {
            self.free_indices.push(index);
        }
    }

    pub fn get(&self, entity: &EntityId) -> Option<&T> {
        self.index_map.get(entity).map(|&index| &self.components[index])
    }

    pub fn get_mut(&mut self, entity: &EntityId) -> Option<&mut T> {
        self.index_map.get(entity).map(|&index| &mut self.components[index])
    }

    pub fn get_by_id(&self, id: ComponentId<T>) -> Option<&T> {
        self.components.get(id.index() as usize)
    }

    pub fn get_by_index(&self, index: usize) -> Option<&T> {
        self.components.get(index)
    }

    pub fn iter_with_index(&self) -> impl Iterator<Item = (&EntityId, usize, &T)> {
        self.index_map
            .iter()
            .map(|(entity, &index)| (entity, index, &self.components[index]))
    }

    pub fn components(&self) -> &Vec<T> {
        &self.components
    }
}
