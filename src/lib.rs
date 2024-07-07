#[macro_export]
macro_rules! expect_or_return {
    (($e:expr),($m:expr)) => {
        match $e {
            Ok(x) => x,
            Err(_) => {
                println!("{}", $m);
                return;
            }
        }
    };
}
