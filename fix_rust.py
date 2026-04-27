import re

with open("tests/raop_compliance.rs", "r") as f:
    c = f.read()

# Replace block formatting with inline formatting using \r\n
c = re.sub(r'let response = format!\(\s+"RTSP/1\.0 200 OK\nCSeq: {}\n\n",\s+cseq\s+\);', r'let response = format!("RTSP/1.0 200 OK\\r\\nCSeq: {}\\r\\n\\r\\n", cseq);', c)

c = re.sub(r'let response = format!\(\s+"RTSP/1\.0 200 OK\nCSeq: {}\nSession: CAFEBABE\nTransport: RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002\n\n",\s+cseq\s+\);', r'let response = format!("RTSP/1.0 200 OK\\r\\nCSeq: {}\\r\\nSession: CAFEBABE\\r\\nTransport: RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002\\r\\n\\r\\n", cseq);', c)

c = re.sub(r'let response = format!\(\s+"RTSP/1\.0 200 OK\nCSeq: {}\nAudio-Latency: 2205\n\n",\s+cseq\s+\);', r'let response = format!("RTSP/1.0 200 OK\\r\\nCSeq: {}\\r\\nAudio-Latency: 2205\\r\\n\\r\\n", cseq);', c)

with open("tests/raop_compliance.rs", "w") as f:
    f.write(c)
