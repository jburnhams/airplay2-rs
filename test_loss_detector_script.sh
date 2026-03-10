# What is this test actually testing?
# It tests the client side of RTP Packet loss detection?
# "Client-side buffering not tested (we're sender not receiver)"
# But AirPlay2 clients are senders... we send audio, receiver buffers. Wait...
# The receiver also sends RTCP back to the client!
# Let's check airplay2-checklist.md
