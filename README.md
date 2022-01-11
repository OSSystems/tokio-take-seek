# tokio-take-seek

This creates offers a wraparound to `tokio::io::Take` that offers a `tokio::io::AsyncSeek`
implementation (as long as the inner handle used implements it as well). The idea is to
use the `Take` implementation as a _upper bound_ and not allow reads beyond that point,
and when `seek` operations are called, the underling limit of how many bytes can still be
read is properly updated.

The provided `AsyncTakeSeekExt` trait offers a `take_with_seek` method for creating the
wrapper over any type that implements `AsyncRead + AsyncSeek + Unpin`.

## Implementation

The current implementation has to do internally two seeks for every `seek` called. The
first `seek` is done with `SeekFrom::current(0)` and it's result is used to determine how
much the stream has moved so the `Take`'s inner limit can be properly updated.

## Usage example:

On this example we use the `take_with_seek` to guarantee that the string won't be read
beyond the desired limit, while still being able to seek over the reader even after the
bound is created.

```Rust
async fn basic_seek_and_reed() {
    let file = tempfile::NamedTempFile::new().unwrap();
    //                                        21v               38v
    fs::write(file.path(), "this will be skipped|this will be read|this will be guarded")
        .await
        .unwrap();

    let mut handle = TakeSeek::new(take::take(fs::File::open(file.path()).await.unwrap(), 38));
    handle.seek(SeekFrom::Start(21)).await.unwrap();

    let mut data = String::default();
    handle.read_to_string(&mut data).await.unwrap();
    assert_eq!(data, "this will be read");
}

``` 
