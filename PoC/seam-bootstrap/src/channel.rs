//! Channel Implementation
//!
//! Channels are the execution path control units with entry and collector paths

use crate::context::{ExecutionContext, ResourceFramePtr};

/// Channel state machine
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Idle, never invoked
    Idle = 0,
    /// Currently executing entry path
    Active = 1,
    /// Entry returned normally
    Returned = 2,
    /// Aborted, in collector
    Aborted = 3,
    /// Collector completed
    Collected = 4,
}

/// Channel metadata
#[repr(C)]
pub struct ChannelMetadata {
    /// Unique channel identifier
    pub channel_id: u32,
    /// Current channel state
    pub state: ChannelState,
    /// Entry function pointer
    pub entry_ptr: Option<unsafe extern "C" fn(*mut ExecutionContext) -> i32>,
    /// Collector function pointer
    pub collector_ptr: Option<unsafe extern "C" fn(*mut ExecutionContext, ResourceFramePtr) -> i32>,
    /// Expected frame size (static from compiler)
    pub frame_size: usize,
    /// Local resource descriptors
    pub local_resources: *const LocalResourceDesc,
    /// Number of local resources
    pub num_resources: u32,
}

/// Descriptor for local resources within a channel
#[repr(C)]
pub struct LocalResourceDesc {
    /// Resource name hash
    pub name_hash: u32,
    /// Offset from frame base
    pub offset: usize,
    /// Size of resource
    pub size: usize,
    /// Cleanup function (if needed)
    pub cleanup_ptr: Option<unsafe extern "C" fn(*mut u8)>,
}

/// Channel representation
pub struct Channel {
    metadata: ChannelMetadata,
}

impl Channel {
    /// Create a new channel
    pub fn new(
        channel_id: u32,
        frame_size: usize,
        entry_ptr: Option<unsafe extern "C" fn(*mut ExecutionContext) -> i32>,
        collector_ptr: Option<unsafe extern "C" fn(*mut ExecutionContext, ResourceFramePtr) -> i32>,
    ) -> Self {
        Channel {
            metadata: ChannelMetadata {
                channel_id,
                state: ChannelState::Idle,
                entry_ptr,
                collector_ptr,
                frame_size,
                local_resources: std::ptr::null(),
                num_resources: 0,
            },
        }
    }

    /// Get channel ID
    #[inline]
    pub fn channel_id(&self) -> u32 {
        self.metadata.channel_id
    }

    /// Get current state
    #[inline]
    pub fn state(&self) -> ChannelState {
        self.metadata.state
    }

    /// Get frame size
    #[inline]
    pub fn frame_size(&self) -> usize {
        self.metadata.frame_size
    }

    /// Invoke channel entry
    ///
    /// Allocates frame, calls entry function, and manages state
    pub unsafe fn invoke(&mut self, ctx: &mut ExecutionContext) -> Result<i32, &'static str> {
        if self.metadata.state != ChannelState::Idle {
            return Err("Channel already active or completed");
        }

        self.metadata.state = ChannelState::Active;

        // Allocate frame
        let _frame_ptr = ctx.frame_push(self.metadata.frame_size)?;

        // Call entry function
        let result = if let Some(entry) = self.metadata.entry_ptr {
            entry(ctx as *mut ExecutionContext)
        } else {
            return Err("No entry function defined");
        };

        self.metadata.state = ChannelState::Returned;
        Ok(result)
    }

    /// Invoke channel collector
    ///
    /// Called when abort occurs in this channel's context
    pub unsafe fn invoke_collector(
        &mut self,
        ctx: &mut ExecutionContext,
        rfp: ResourceFramePtr,
    ) -> Result<i32, &'static str> {
        if self.metadata.state != ChannelState::Aborted {
            self.metadata.state = ChannelState::Aborted;
        }

        let result = if let Some(collector) = self.metadata.collector_ptr {
            collector(ctx as *mut ExecutionContext, rfp)
        } else {
            0 // No collector, default recovery
        };

        self.metadata.state = ChannelState::Collected;
        Ok(result)
    }

    /// Get metadata reference
    #[inline]
    pub fn metadata(&self) -> &ChannelMetadata {
        &self.metadata
    }

    /// Get metadata mutable reference
    #[inline]
    pub fn metadata_mut(&mut self) -> &mut ChannelMetadata {
        &mut self.metadata
    }
}

/// ChannelBuilder for fluent API
pub struct ChannelBuilder {
    channel_id: u32,
    frame_size: usize,
    entry_ptr: Option<unsafe extern "C" fn(*mut ExecutionContext) -> i32>,
    collector_ptr: Option<unsafe extern "C" fn(*mut ExecutionContext, ResourceFramePtr) -> i32>,
}

impl ChannelBuilder {
    /// Create new builder
    pub fn new(channel_id: u32) -> Self {
        ChannelBuilder {
            channel_id,
            frame_size: 0,
            entry_ptr: None,
            collector_ptr: None,
        }
    }

    /// Set frame size
    pub fn frame_size(mut self, size: usize) -> Self {
        self.frame_size = size;
        self
    }

    /// Set entry function
    pub fn entry(mut self, entry: unsafe extern "C" fn(*mut ExecutionContext) -> i32) -> Self {
        self.entry_ptr = Some(entry);
        self
    }

    /// Set collector function
    pub fn collector(
        mut self,
        collector: unsafe extern "C" fn(*mut ExecutionContext, ResourceFramePtr) -> i32,
    ) -> Self {
        self.collector_ptr = Some(collector);
        self
    }

    /// Build the channel
    pub fn build(self) -> Channel {
        Channel::new(
            self.channel_id,
            self.frame_size,
            self.entry_ptr,
            self.collector_ptr,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_creation() {
        let channel = ChannelBuilder::new(1)
            .frame_size(256)
            .build();

        assert_eq!(channel.channel_id(), 1);
        assert_eq!(channel.frame_size(), 256);
        assert_eq!(channel.state(), ChannelState::Idle);
    }

    #[test]
    fn test_channel_builder() {
        let channel = ChannelBuilder::new(42)
            .frame_size(512)
            .build();

        assert_eq!(channel.channel_id(), 42);
        assert_eq!(channel.frame_size(), 512);
    }
}
