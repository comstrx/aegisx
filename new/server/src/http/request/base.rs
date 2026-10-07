use http::Uri;
use http::uri::PathAndQuery;

use super::arch::Request;

impl Request {

    pub fn origin_form ( uri: &Uri ) -> Option<Uri> {

        if uri.scheme().is_none() && uri.authority().is_none() { return None; }

        let target = uri.path_and_query().cloned().unwrap_or_else(|| PathAndQuery::from_static("/"));

        Uri::builder().path_and_query(target).build().ok()

    }

    pub fn target ( uri: &Uri ) -> &str {

        uri.path_and_query().map(PathAndQuery::as_str).unwrap_or("/")

    }

}
