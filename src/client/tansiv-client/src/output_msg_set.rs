use crate::connector::SendPacketBuilder;
use std::cell::UnsafeCell;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub enum Error {
    NoSlotAvailable
}

type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            Error::NoSlotAvailable => "No slot available",
        };
        write!(f, "{}", msg)
    }
}

impl std::error::Error for Error {}

pub type OutputMsg = SendPacketBuilder;

#[derive(Debug)]
pub struct OutputMsgSet {
    slots: Vec<UnsafeCell<Option<OutputMsg>>>,
    slot_busy: Vec<AtomicBool>,
    slot_valid: Vec<AtomicBool>,
}

pub struct OutputMsgDrain<'a> {
    msg_set: &'a OutputMsgSet,
    index: usize,
}

impl<'a> Iterator for OutputMsgDrain<'a> {
    type Item = OutputMsg;

    fn next<'b>(&'b mut self) -> Option<OutputMsg> {
        let msg_set = self.msg_set;
        let num_slots = msg_set.slots.len();
        let next_index = self.index;
        for index in next_index..num_slots {
            let val = msg_set.take_slot(index);
            if val.is_some() {
                self.index = index + 1;
                // let val = val.unwrap();
                // return Some((val.send_time, val.src, val.dst, val.payload));
                return val;
            }
        }

        None
    }
}

impl OutputMsgSet {
    pub fn new(num_slots: usize) -> OutputMsgSet {
        let mut slots: Vec<UnsafeCell<Option<OutputMsg>>> = Vec::with_capacity(num_slots);
        slots.resize_with(num_slots, || UnsafeCell::new(None));
        let mut slot_busy: Vec<AtomicBool> = Vec::with_capacity(num_slots);
        slot_busy.resize_with(num_slots, Default::default);
        let mut slot_valid: Vec<AtomicBool> = Vec::with_capacity(num_slots);
        slot_valid.resize_with(num_slots, Default::default);

        OutputMsgSet {
            slots: slots,
            slot_busy: slot_busy,
            slot_valid: slot_valid,
        }
    }

    pub fn insert(&self, output_msg: OutputMsg) -> Result<()> {
        for (idx, slot) in self.slot_busy.iter().enumerate() {
            if !slot.swap(true, Ordering::AcqRel) {
                unsafe {
                    self.slots[idx].get().replace(Some(output_msg));
                }
                self.slot_valid[idx].store(true, Ordering::Release);
                return Ok(());
            }
        }
        Err(Error::NoSlotAvailable)
    }

    pub fn drain<'a>(&'a self) -> OutputMsgDrain<'a> {
        OutputMsgDrain {
            msg_set: self,
            index: 0,
        }
    }

    fn take_slot(&self, index: usize) -> Option<OutputMsg> {
        if let Some(slot) = self.slot_valid.get(index) {
            if slot.load(Ordering::Acquire) {
                let output_msg = unsafe { self.slots[index].get().replace(None) };

                slot.store(false, Ordering::Release);
                self.slot_busy[index].store(false, Ordering::Release);

                return output_msg;
            }
        }

        None
    }
}

unsafe impl Send for OutputMsgSet {}
unsafe impl Sync for OutputMsgSet {}
