use crate::holder::Holder;

pub fn stamp(holder: &Holder) -> u64 {
    holder.clock.tick_forward()
}
