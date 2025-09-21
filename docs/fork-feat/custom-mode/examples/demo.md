---
kind: persistent
default_enabled: true
display_name: Demo
variables:
  - name: who
    enum: [张三, bob, carol]
    default: 张三
  - name: friend
---

用户叫 {{who}}, 有个朋友叫{{friend}}

