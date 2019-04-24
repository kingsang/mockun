# mockun

- mockサーバです
- Rustの勉強がてら作成

## つかいかた

```
mockun -p 6789 -h custom-response-header /patha:./response.json /pathb:./response.text
```

- -pでポート指定(option)
- -hでレスポンスに載せたいヘッダーの追加
- <用意したいaccess path>:<そのpathから返したいresponse bodyが書かれたファイルパス>形式で複数指定可能

