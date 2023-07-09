# Generational Arena DOM

This is an implementation of the DOM used in `html5ever` using [`generational_indextree`](https://gitlab.com/barry.van.acker/generational-indextree), an implementation of [indextree](https://github.com/saschagrunert/indextree) using [generational arenas](https://github.com/fitzgen/generational-arena).

Using an indextree for the DOM makes mutation much simpler, as the docs for indextree state:

> This arena tree structure is using just a single Vec and numerical identifiers (indices in the vector) instead of reference counted pointers like. This means there is no RefCell and mutability is handled in a way much more idiomatic to Rust through unique (&mut) access to the arena.

However, indextree suffers from the [ABA problem](https://en.wikipedia.org/wiki/ABA_problem), which we can solve via using generational-arenas instead of `Vec` based arenas.
