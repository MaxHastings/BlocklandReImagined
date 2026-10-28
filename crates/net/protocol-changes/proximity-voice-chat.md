Voice datagrams (`crate::voice`): a datagram starting with the byte 0xC1 is
voice, never MessagePack. Clients send frames upstream; the host stamps the
speaker's owner and position and relays each frame only to players within
`voice::RANGE` of the speaker.
