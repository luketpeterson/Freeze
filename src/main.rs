use freeze::BumpAllocRef;

/*
fn main() {
  let a = BumpAllocRef::new_with_address_space(40);

  let s = {
    let mut v = a.top();
    v.extend((0..2_000_000_000).map(|x| x as u8));
    v.freeze()
  };

  println!("initialized");

  unsafe {
    libc::sleep(20);
  }

  println!("shrinking");
  a.shrink_to_allocated();
  println!("shrunk");

  unsafe {
    libc::sleep(20);
  }

  println!("checking");
  s.iter().copied().enumerate().for_each(|(x, y)| assert_eq!(x as u8, y));
  println!("checked");
}
*/