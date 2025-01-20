## <img src="assets/bigworlds-logo.png" width="300">

[![Static Badge](https://img.shields.io/badge/discord-server-blue)](https://discord.gg/SHQ3rYvn3B)

<!-- cargo-rdme start -->


`bigworlds` is a novel framework for assembling and running large simulation
models.

It tries to solve the problem of dynamic yet efficient distribution of discrete
simulation elements over many machines. 


### Quick-start

Use `bigworlds-cli` to run a model locally:

```
cargo run --release -- run examples/flock/model.toml
```

Networked cluster can be assembled from multiple worker processes:

```
# machine 1
cargo run --release -- worker --leader

# machine 2
cargo run --release -- worker 
```


### Goals

Provide a batteries-included solution for creating large virtual worlds with
millions of persistent entities. Provide a way for creating *multiverse*
systems made up of model-compatible worlds.

Allow building massive multiplayer experiences with thousands of concurrent
players at relatively high degree of fidelity. Allow operating them at
a fraction of the cost required by currently available closed-source systems.

Make assembling and running large-scale socio-economic simulations feasible
on off-the-shelf hardware. Make model definition process scalable, allow
multiple ways for importing data and defining logic.


### Design

Focus is on discrete entity-based models, with capability for driving logic
based on both global events and regular time cycles.

Entities are generally composed of components defining the data they store.
Components also potentially define the logic that should be applied to given
entities.

Storage is optimized for entity-level retrieval and transfer between nodes.

The runtime is kept as generic as possible, allowing for wide variety of logic
and compute configurations. This includes dynamic cluster enlargement and
changes to the simulation model at runtime.

Actors with complex sets of behaviors can be constructed and deployed into the
simulation world at any point, including by other actors at runtime.

Interesting recursion patterns are possible, with actors being able to start up
`bigworlds` simulations themselves. 


<!-- cargo-rdme end -->
