macro_rules! not_last {
    ($cb:ident($($args:tt),*) => $($elems:ident,)+) => {
        not_last!($cb($($args),*) => $($elems,)+;);
    };
    ($cb:ident($($args:tt),*) => $last:ident,; $($elems:ident,)*) => {
        $cb!($($args),*; $($elems,)*; $last);
    };
    ($cb:ident($($args:tt),*) => $elem:ident, $($remaining:ident,)*; $($elems:ident,)*) => {
        not_last!($cb($($args),*) => $($remaining,)*; $($elems,)* $elem,);
    };
}

macro_rules! all_methods {
    ($cb:ident) => {
        $cb!(get GET);
        $cb!(post POST);
        $cb!(put PUT);
        $cb!(delete DELETE);
        $cb!(head HEAD);
        $cb!(options OPTIONS);
        $cb!(connect CONNECT);
        $cb!(patch PATCH);
        $cb!(trace TRACE);
    };
}
