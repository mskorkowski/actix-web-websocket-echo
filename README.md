# actix-web-websocket-echo

"Better" websocket echo server

## Fixes

### 5 Fragmentation

- 5.3
- 5.4
- 5.5
- 5.6
- 5.7
- 5.8
- 5.18
- 5.19
- 5.20

### 6 UTF-8 Handling

#### 6.1 Valid UTF-8 with zero payload fragments

- 6.1.2
- 6.1.3

#### 6.2 Valid UTF-8 unfragmented, fragmented on code-points and within code-points

- 6.2.2
- 6.2.3
- 6.2.4

#### 6.3 Invalid UTF-8 differently fragmented

- 6.3.2

### 6.4 Fail fast on invalid UTF-8

- 6.4.1
- 6.4.2
- 6.4.3 - non strict
- 6.4.4 - non strict

### 7 Close Handling

- 7.9.1
- 7.9.2
- 7.9.3
- 7.9.4
- 7.9.5
- 7.9.6
- 7.9.7
- 7.9.8
- 7.9.9

## From non strict to strict

- 5.15

## Unsolvable on echo example side

### 3 Reserved bits

On the echo example side it would require the access to the reserved bits of the frame

- 3.1
- 3.2
- 3.3
- 3.4
- 3.5
- 3.6

### 7.5 Close Handling

7.5.1 - By spec in such a case connection should be closed with `CloseCode::Protocol`.

When close frame with invalid utf-8 sequence is received then Codec is using `String::from_utf8_lossy` on the description. This make it impossible to detect on the echo server side wherever the data sent were valid or not, since `StreamHandler::handle` receives valid String as part of the `CloseReason::description`.

One way to solve it would be to fail on the Codec side, but this might be problematic since that would make it harder to close the ws resources.

Another way would be to change `CloseReason::description` from `Option<String>` to `Option<ByteString>` and made it user responsibility to detect it and close the connection.
