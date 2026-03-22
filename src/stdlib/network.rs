use crate::error::{Error, Result};
use crate::value::Value;

pub fn call(function: &str, _args: Vec<Value>) -> Result<Value> {
    match function {
        "get" | "fetch" => Err(Error::Runtime("Network module requires async runtime".to_string())),
        "post" => Err(Error::Runtime("Network module requires async runtime".to_string())),
        "request" => Err(Error::Runtime("Network module requires async runtime".to_string())),
        "download" => Err(Error::Runtime("Network module requires async runtime".to_string())),
        "upload" => Err(Error::Runtime("Network module requires async runtime".to_string())),
        _ => Err(Error::Runtime(format!("Unknown network function '{}'", function))),
    }
}

pub fn call_async(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "get" => http_get(args),
        "post" => http_post(args),
        "delete" => http_delete(args),
        "put" => http_put(args),
        "patch" => http_patch(args),
        "download" => download_file(args),
        "upload" => upload_file(args),
        "parse_url" => parse_url(args),
        "encode_url" => encode_url(args),
        "decode_url" => decode_url(args),
        "get_headers" => get_headers(args),
        "get_status" => get_status(args),
        _ => Err(Error::Runtime(format!("Unknown network function '{}'", function))),
    }
}

fn http_get(args: Vec<Value>) -> Result<Value> {
    let url = expect_text(args.first())?;
    
    #[cfg(feature = "network")]
    {
        // TODO: Implement with reqwest or ureq
        Err(Error::Runtime("HTTP not implemented yet".to_string()))
    }
    
    #[cfg(not(feature = "network"))]
    {
        let _ = url;
        Err(Error::Runtime("HTTP support not compiled in".to_string()))
    }
}

fn http_post(args: Vec<Value>) -> Result<Value> {
    let _ = args;
    #[cfg(feature = "network")]
    {
        Err(Error::Runtime("HTTP not implemented yet".to_string()))
    }
    #[cfg(not(feature = "network"))]
    {
        Err(Error::Runtime("HTTP support not compiled in".to_string()))
    }
}

fn http_delete(args: Vec<Value>) -> Result<Value> {
    let _ = args;
    #[cfg(feature = "network")]
    {
        Err(Error::Runtime("HTTP not implemented yet".to_string()))
    }
    #[cfg(not(feature = "network"))]
    {
        Err(Error::Runtime("HTTP support not compiled in".to_string()))
    }
}

fn http_put(args: Vec<Value>) -> Result<Value> {
    let _ = args;
    #[cfg(feature = "network")]
    {
        Err(Error::Runtime("HTTP not implemented yet".to_string()))
    }
    #[cfg(not(feature = "network"))]
    {
        Err(Error::Runtime("HTTP support not compiled in".to_string()))
    }
}

fn http_patch(args: Vec<Value>) -> Result<Value> {
    let _ = args;
    #[cfg(feature = "network")]
    {
        Err(Error::Runtime("HTTP not implemented yet".to_string()))
    }
    #[cfg(not(feature = "network"))]
    {
        Err(Error::Runtime("HTTP support not compiled in".to_string()))
    }
}

fn download_file(args: Vec<Value>) -> Result<Value> {
    let _ = args;
    #[cfg(feature = "network")]
    {
        Err(Error::Runtime("Download not implemented yet".to_string()))
    }
    #[cfg(not(feature = "network"))]
    {
        Err(Error::Runtime("Download support not compiled in".to_string()))
    }
}

fn upload_file(args: Vec<Value>) -> Result<Value> {
    let _ = args;
    #[cfg(feature = "network")]
    {
        Err(Error::Runtime("Upload not implemented yet".to_string()))
    }
    #[cfg(not(feature = "network"))]
    {
        Err(Error::Runtime("Upload support not compiled in".to_string()))
    }
}

fn parse_url(args: Vec<Value>) -> Result<Value> {
    let url = expect_text(args.first())?;
    
    use std::collections::HashMap;
    let mut parts = HashMap::new();
    
    if let Some((scheme, rest)) = url.split_once("://") {
        parts.insert("scheme".to_string(), Value::Text(scheme.to_string()));
        
        if let Some((host, path)) = rest.split_once('/') {
            parts.insert("host".to_string(), Value::Text(host.to_string()));
            parts.insert("path".to_string(), Value::Text(format!("/{}", path)));
        } else {
            parts.insert("host".to_string(), Value::Text(rest.to_string()));
            parts.insert("path".to_string(), Value::Text("/".to_string()));
        }
    } else {
        parts.insert("path".to_string(), Value::Text(url));
    }
    
    Ok(Value::Record(parts))
}

fn encode_url(args: Vec<Value>) -> Result<Value> {
    let url = expect_text(args.first())?;
    
    let encoded: String = url.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect();
    
    Ok(Value::Text(encoded))
}

fn decode_url(args: Vec<Value>) -> Result<Value> {
    let url = expect_text(args.first())?;
    
    let mut decoded = String::new();
    let mut chars = url.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                decoded.push(byte as char);
            } else {
                decoded.push('%');
                decoded.push_str(&hex);
            }
        } else if c == '+' {
            decoded.push(' ');
        } else {
            decoded.push(c);
        }
    }
    
    Ok(Value::Text(decoded))
}

fn get_headers(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("get_headers requires an HTTP response".to_string()))
}

fn get_status(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("get_status requires an HTTP response".to_string()))
}

fn expect_text(value: Option<&Value>) -> Result<String> {
    match value {
        Some(Value::Text(s)) => Ok(s.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected text, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}
