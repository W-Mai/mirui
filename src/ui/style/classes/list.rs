use alloc::{borrow::Cow, string::String, vec::Vec};

/// Ordered class names for one widget. Later names can override earlier ones.
#[derive(Clone, Default)]
pub struct ClassList(Names);

#[derive(Clone, Default)]
enum Names {
    #[default]
    Empty,
    Static(&'static [&'static str]),
    Inline([&'static str; 3], u8),
    Owned(Vec<Cow<'static, str>>),
}

impl ClassList {
    fn len(&self) -> usize {
        match &self.0 {
            Names::Empty => 0,
            Names::Static(names) => names.len(),
            Names::Inline(_, len) => usize::from(*len),
            Names::Owned(names) => names.len(),
        }
    }

    pub(super) fn iter(&self) -> impl DoubleEndedIterator<Item = &str> {
        let (borrowed, owned) = match &self.0 {
            Names::Empty => (&[][..], &[][..]),
            Names::Static(names) => (*names, &[][..]),
            Names::Inline(names, len) => (&names[..usize::from(*len)], &[][..]),
            Names::Owned(names) => (&[][..], names.as_slice()),
        };
        borrowed
            .iter()
            .copied()
            .chain(owned.iter().map(Cow::as_ref))
    }
}

impl PartialEq for ClassList {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for ClassList {}

impl From<&'static [&'static str]> for ClassList {
    fn from(names: &'static [&'static str]) -> Self {
        Self(Names::Static(names))
    }
}

impl<const N: usize> From<&'static [&'static str; N]> for ClassList {
    fn from(names: &'static [&'static str; N]) -> Self {
        Self::from(names.as_slice())
    }
}

impl<T: Into<Cow<'static, str>>, const N: usize> From<[T; N]> for ClassList {
    fn from(names: [T; N]) -> Self {
        names.into_iter().map(Into::into).collect()
    }
}

impl<T: Into<Cow<'static, str>>> From<Vec<T>> for ClassList {
    fn from(names: Vec<T>) -> Self {
        Self(Names::Owned(names.into_iter().map(Into::into).collect()))
    }
}

impl From<&'static str> for ClassList {
    fn from(name: &'static str) -> Self {
        [name].into()
    }
}

impl From<String> for ClassList {
    fn from(name: String) -> Self {
        [name].into()
    }
}

impl From<Cow<'static, str>> for ClassList {
    fn from(name: Cow<'static, str>) -> Self {
        [name].into()
    }
}

impl FromIterator<Cow<'static, str>> for ClassList {
    fn from_iter<T: IntoIterator<Item = Cow<'static, str>>>(names: T) -> Self {
        let mut names = names.into_iter();
        let mut inline = [""; 3];
        for index in 0..inline.len() {
            match names.next() {
                None => return Self(Names::Inline(inline, index as u8)),
                Some(Cow::Borrowed(name)) => inline[index] = name,
                Some(name) => return owned(&inline[..index], name, names),
            }
        }
        match names.next() {
            None => Self(Names::Inline(inline, inline.len() as u8)),
            Some(name) => owned(&inline, name, names),
        }
    }
}

fn owned(
    prefix: &[&'static str],
    first: Cow<'static, str>,
    rest: impl Iterator<Item = Cow<'static, str>>,
) -> ClassList {
    let names = prefix
        .iter()
        .copied()
        .map(Cow::Borrowed)
        .chain(core::iter::once(first))
        .chain(rest)
        .collect();
    ClassList(Names::Owned(names))
}

trait ClassName {
    fn into_name(self) -> Option<Cow<'static, str>>;
}

macro_rules! class_name {
    ($name:ty) => {
        impl ClassName for $name {
            fn into_name(self) -> Option<Cow<'static, str>> {
                Some(self.into())
            }
        }
    };
}

class_name!(&'static str);
class_name!(String);
class_name!(Cow<'static, str>);

impl<T: ClassName> ClassName for Option<T> {
    fn into_name(self) -> Option<Cow<'static, str>> {
        self.and_then(ClassName::into_name)
    }
}

impl<T: ClassName> From<Option<T>> for ClassList {
    fn from(name: Option<T>) -> Self {
        name.into_name().into_iter().collect()
    }
}

macro_rules! class_tuple {
    ($($item:ident),+) => {
        impl<$($item: ClassName),+> From<($($item,)+)> for ClassList {
            fn from(names: ($($item,)+)) -> Self {
                #[allow(non_snake_case)]
                let ($($item,)+) = names;
                [$($item.into_name(),)+].into_iter().flatten().collect()
            }
        }
    };
}

class_tuple!(A);
class_tuple!(A, B);
class_tuple!(A, B, C);
class_tuple!(A, B, C, D);
class_tuple!(A, B, C, D, E);
class_tuple!(A, B, C, D, E, F);
