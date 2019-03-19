extern crate tch;
use tch::{kind, Tensor, Device, Kind};

fn grad_example() {
    let mut x = Tensor::from(2.0).set_requires_grad(true);
    let y = &x * &x + &x + 36;
    println!("{}", y.double_value(&[]));
    x.zero_grad();
    y.backward();
    let dy_over_dx = x.grad();
    println!("{}", dy_over_dx.double_value(&[]));
}

fn main() {
    let t = Tensor::int_vec(&[3, 1, 4, 1, 5]);
    t.print();
    let t = Tensor::randn(&[5, 4], kind::FLOAT_CPU);
    t.print();
    let t2 = Tensor::randn(&[20, 144, 13, 2], (Kind::Float, Device::cuda_if_available()));
    println!("t2: {}.", t2);
    (&t + 1.5).print();
    (&t + 2.5).print();
    let mut t = Tensor::float_vec(&[1.1, 2.1, 3.1]);
    t += 42;
    t.print();
    println!("{:?} {}", t.size(), t.double_value(&[1]));
    grad_example();
}
