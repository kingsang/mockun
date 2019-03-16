use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;
use std::io::BufReader;
use std::fs::File;
use std::thread;
use std::sync::Arc;

const USAGE: &'static str = r#"
Usage:
  mockun [-p <port>] /path:/xxx/file ...

Example:
  mockun -p 6789 /aa:./response.json /aa/bb:/response.text
"#;

fn main() {
  // 引数のパースから、ファイルの中身のレスポンスを作る
  let args = parse_args();
  let host = format!("127.0.0.1:{}", args.port_opt.unwrap_or("7878".to_string()));

  let _path_and_response_bodies = make_responses(&args.path_and_file_names);
  let all_paths = _path_and_response_bodies.iter().map(|pr| " 🎯 ".to_string() + pr.path.as_str()).collect::<Vec<String>>();

  println!("mockun start!!\n 👉 {}\npaths are ...\n{}", host, all_paths.join("\n"));

  // request handlerをマルチスレッドに実行するのでArcでwrap
  let path_and_response_bodies = Arc::new(_path_and_response_bodies);

  let listener = TcpListener::bind(host).unwrap();
  for stream in listener.incoming() {
    let stream = stream.unwrap();
    let shared = path_and_response_bodies.clone();
    thread::spawn(move || {
      handle_connection(stream, &shared);
    });
  }
  drop(listener);
}

fn handle_connection(mut stream: TcpStream, path_and_responses: &Vec<PathAndResponse>) -> () {
  // requestヘッダのパース
  let mut _stream = BufReader::new(stream.try_clone().unwrap());
  let mut first_line = String::new();
  _stream.read_line(&mut first_line).unwrap();
  let mut params = first_line.split_whitespace();
  let _method = params.next();
  let path = params.next().unwrap();

  // リクエストパスとキャッシュ上の設定パスを比較して一致した物のレスポンスを返す
  let path_and_response_opt = &path_and_responses.iter().find(|pr| pr.path == path);

  let [body, content_type] = path_and_response_opt
    .map(|pr| [pr.response_body.clone(), pr.content_type.clone()])
    .unwrap_or_else(|| ["nothing response is set!".to_string(), "text/plain".to_string()]);

  writeln!(stream, "HTTP/1.1 200 OK").unwrap();
  writeln!(stream, "Content-Type: {}; charset=UTF-8", content_type).unwrap();
  writeln!(stream, "Content-Length: {}", body.len()).unwrap();
  writeln!(stream, "Server: {}", "mockun").unwrap();
  writeln!(stream).unwrap();
  writeln!(stream, "{}", body).unwrap();
  stream.flush().unwrap();
}

fn make_responses(path_and_file_names: &Vec<PathAndFileName>) -> Vec<PathAndResponse> {
  path_and_file_names.iter().map(|pf| {
    let file_name = pf.file_name.clone();

    let file_extension = *file_name.split(".").collect::<Vec<&str>>().iter().last().unwrap_or(&"text");
    let content_type = match file_extension {
      "json" => "application/json",
      "js" => "application/javascript",
      "text" => "text/plain",
      "html" => "text/html",
      _ => "text/plain"
    }.to_string();

    // mapのclosureの中で ? 使うとclosureがResultを返すべき関数とみなされエラーになるので仕方なくいちいちpanicする事にする
    let mut contents = String::new();
    let mut file = File::open(file_name).unwrap_or_else(|err| panic!(err.to_string() + USAGE));
    file.read_to_string(&mut contents).unwrap_or_else(|err| panic!(err.to_string() + USAGE));

    PathAndResponse {
      path: pf.path.clone(),
      response_body: contents,
      content_type,
    }
  }).collect()
}

fn parse_args() -> Args {
  let mut args_vec: Vec<String> = std::env::args().collect();

  // 最初の要素は実行binaryパスが入るので除外
  args_vec.remove(0);

  // portオプションのパース
  let port_op_index_opt = args_vec.iter().position(|r| r.starts_with("-p"));
  let port_opt: Option<String> = port_op_index_opt.map(|index| {
    // --p 7878 という形で入ってくるので番号だけ取り出したい...これきれいに書けんのかな？
    let flg_and_port_indexes = std::ops::Range { start: index, end: index + 2 };
    let flg_and_port = args_vec.drain(flg_and_port_indexes).collect::<Vec<String>>();
    if flg_and_port.len() < 2 { panic!(USAGE); }
    flg_and_port.get(1).unwrap().to_string()
  });

  // パスとレスポンスのパース
  let path_and_file_names: Vec<PathAndFileName> = args_vec.iter().map(|pf| {
    let pair_vec: Vec<&str> = pf.split(":").collect();
    // /path:./filepath という形で入ってくるので2要素必須
    if pair_vec.len() != 2 { panic!(USAGE); }
    PathAndFileName {
      path: pair_vec.get(0).unwrap().to_string(),
      file_name: pair_vec.get(1).unwrap().to_string(),
    }
  }).collect();

  if path_and_file_names.len() == 0 {
    panic!(USAGE);
  }

  Args { path_and_file_names, port_opt }
}

struct Args {
  path_and_file_names: Vec<PathAndFileName>,
  port_opt: Option<String>,
}

struct PathAndFileName {
  path: String,
  file_name: String,
}

struct PathAndResponse {
  path: String,
  response_body: String,
  content_type: String,
}