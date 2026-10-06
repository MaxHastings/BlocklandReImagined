//! Buffers created with their contents, like wgpu's `create_buffer_init`,
//! except on a lost device. Once the GPU drops out (a driver reset, an
//! integrated GPU running out of memory), wgpu hands back invalid buffers,
//! and `create_buffer_init` panics mapping one: the game crashed on its
//! next upload instead of reaching the frame that notices the loss and
//! restarts the renderer. Here the invalid buffer is returned unfilled;
//! nothing draws with it before that restart replaces it.
pub trait BufferInit {
    fn buffer_init(&self, descriptor: &wgpu::util::BufferInitDescriptor<'_>) -> wgpu::Buffer;
}

impl BufferInit for wgpu::Device {
    fn buffer_init(&self, descriptor: &wgpu::util::BufferInitDescriptor<'_>) -> wgpu::Buffer {
        let unpadded = descriptor.contents.len() as wgpu::BufferAddress;
        if unpadded == 0 {
            return self.create_buffer(&wgpu::BufferDescriptor {
                label: descriptor.label,
                size: 0,
                usage: descriptor.usage,
                mapped_at_creation: false,
            });
        }
        // Mapped buffers are sized in whole copy units, as wgpu's own does.
        let align = wgpu::COPY_BUFFER_ALIGNMENT - 1;
        let size = ((unpadded + align) & !align).max(wgpu::COPY_BUFFER_ALIGNMENT);
        let buffer = self.create_buffer(&wgpu::BufferDescriptor {
            label: descriptor.label,
            size,
            usage: descriptor.usage,
            mapped_at_creation: true,
        });
        if let Ok(mut range) = buffer.get_mapped_range_mut(..) {
            range
                .slice(..unpadded as usize)
                .copy_from_slice(descriptor.contents);
            drop(range);
            buffer.unmap();
        }
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Option<wgpu::Device> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(adapter.request_device(&Default::default()))
            .ok()
            .map(|(device, _)| device)
    }

    /// A lost GPU hands back invalid buffers; filling one must not panic.
    #[test]
    fn a_lost_device_gets_an_unfilled_buffer_instead_of_a_panic() {
        let Some(device) = device() else {
            eprintln!("No GPU adapter; skipped");
            return;
        };
        device.destroy();
        let descriptor = wgpu::util::BufferInitDescriptor {
            label: Some("after loss"),
            contents: &[1, 2, 3, 4, 5],
            usage: wgpu::BufferUsages::UNIFORM,
        };
        // wgpu's own helper is what crashed a joining player's game.
        let wgpu_helper = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            use wgpu::util::DeviceExt;
            #[allow(clippy::disallowed_methods)]
            device.create_buffer_init(&descriptor)
        }));
        assert!(wgpu_helper.is_err());
        let _ = device.buffer_init(&descriptor);
    }

    #[test]
    fn contents_are_padded_to_whole_copy_units() {
        let Some(device) = device() else {
            eprintln!("No GPU adapter; skipped");
            return;
        };
        let buffer = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("padded"),
            contents: &[1, 2, 3, 4, 5],
            usage: wgpu::BufferUsages::UNIFORM,
        });
        assert_eq!(buffer.size(), 8);
        let empty = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("empty"),
            contents: &[],
            usage: wgpu::BufferUsages::VERTEX,
        });
        assert_eq!(empty.size(), 0);
    }
}
