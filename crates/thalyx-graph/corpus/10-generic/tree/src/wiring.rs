use crate::holder::Holder;

pub fn wire() -> Holder<crate::widget::Widget> {
    Holder {
        inner: crate::widget::Widget,
    }
}
