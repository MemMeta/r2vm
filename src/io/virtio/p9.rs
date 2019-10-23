use super::{Device, DeviceId, Queue};
use byteorder::{WriteBytesExt, LE};
use p9::serialize::{Fcall, Serializable};
use p9::{P9Handler, Passthrough};

use std::io::{Seek, SeekFrom};

/// Feature bit indicating presence of mount tag
const VIRTIO_9P_MOUNT_TAG: u32 = 1;

pub struct P9 {
    status: u32,
    queue: Queue,
    config: Box<[u8]>,
    handler: P9Handler<Passthrough>,
    irq: u32,
}

impl P9 {
    pub fn new(irq: u32, mount_tag: &str, path: &std::path::Path) -> P9 {
        // Config space is composed of u16 length followed by the tag bytes
        let config = {
            let tag_len = mount_tag.len();
            assert!(tag_len <= u16::max_value() as usize);
            let mut config = Vec::with_capacity(tag_len + 2);
            config.push(tag_len as u8);
            config.push((tag_len >> 8) as u8);
            config.extend_from_slice(mount_tag.as_bytes());
            config
        };

        P9 {
            status: 0,
            queue: Queue::new(),
            config: config.into_boxed_slice(),
            handler: P9Handler::new(Passthrough::new(path).unwrap()),
            irq,
        }
    }
}

impl Device for P9 {
    fn device_id(&self) -> DeviceId {
        DeviceId::P9
    }
    fn device_feature(&self) -> u32 {
        VIRTIO_9P_MOUNT_TAG
    }
    fn driver_feature(&mut self, _value: u32) {}
    fn get_status(&self) -> u32 {
        self.status
    }
    fn set_status(&mut self, status: u32) {
        self.status = status
    }
    fn config_space(&self) -> &[u8] {
        &self.config
    }
    fn num_queues(&self) -> usize {
        1
    }
    fn with_queue(&mut self, _idx: usize, f: &mut dyn FnMut(&mut Queue)) {
        f(&mut self.queue)
    }
    fn reset(&mut self) {
        self.status = 0;
    }
    fn notify(&mut self, _idx: usize) {
        while let Ok(Some(mut buffer)) = self.queue.try_take() {
            let (mut reader, mut writer) = buffer.reader_writer();

            reader.seek(SeekFrom::Start(4)).unwrap();
            let (tag, fcall) = <(u16, Fcall)>::decode(&mut reader).unwrap();

            trace!(target: "9p", "received {}, {:?}", tag, fcall);
            let resp = self.handler.handle_fcall(fcall);
            trace!(target: "9p", "send {}, {:?}", tag, resp);

            writer.seek(SeekFrom::Start(4)).unwrap();
            (tag, resp).encode(&mut writer).unwrap();
            let size = writer.seek(SeekFrom::Current(0)).unwrap();
            writer.seek(SeekFrom::Start(0)).unwrap();
            writer.write_u32::<LE>(size as u32).unwrap();

            unsafe {
                self.queue.put(buffer);
            }
        }

        // TODO
        crate::emu::PLIC.lock().trigger(self.irq);
    }
}
