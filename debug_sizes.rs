fn main() {
    println!("Memory sizes:");
    println!("GhostCell<i32>: {} bytes", std::mem::size_of::<halo::GhostCell<i32>>());
    println!("Cell<i32>: {} bytes", std::mem::size_of::<std::cell::Cell<i32>>());
    println!("GhostRefCell<i32>: {} bytes", std::mem::size_of::<halo::GhostRefCell<i32>>());
    println!("RefCell<i32>: {} bytes", std::mem::size_of::<std::cell::RefCell<i32>>());
    println!("GhostUnsafeCell<i32>: {} bytes", std::mem::size_of::<halo::GhostUnsafeCell<i32>>());
    println!("UnsafeCell<i32>: {} bytes", std::mem::size_of::<std::cell::UnsafeCell<i32>>());
    println!("GhostLazyCell<i32>: {} bytes", std::mem::size_of::<halo::GhostLazyCell<i32>>());
    println!("GhostOnceCell<i32>: {} bytes", std::mem::size_of::<halo::GhostOnceCell<i32>>());
    println!("OnceCell<i32>: {} bytes", std::mem::size_of::<std::cell::OnceCell<i32>>());
}






