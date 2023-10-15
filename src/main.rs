use async_trait::async_trait;
use axum::http::Request;
use axum::{body::Body, http::request::Parts};
use frunk::hlist::{HCons, HNil};
use hyper::body::Bytes;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[async_trait(?Send)]
trait Extractor {
    type Output;
    async fn extract(&self, request: &Parts, body: &Bytes) -> Self::Output;
}

struct PathExtractor<T>(fn(&Parts) -> T);

struct BodyExtractor<T>(fn(&Bytes) -> T)
where
    T: DeserializeOwned;

#[async_trait(?Send)]
impl<T> Extractor for PathExtractor<T> {
    type Output = T;
    async fn extract(&self, request: &Parts, _body: &Bytes) -> T {
        self.0(request)
    }
}

#[async_trait(?Send)]
impl<T> Extractor for BodyExtractor<T>
where
    T: DeserializeOwned,
{
    type Output = T;
    async fn extract(&self, _request: &Parts, body: &Bytes) -> T {
        let result: T = serde_json::from_slice(body).unwrap();
        result
    }
}

fn empty_endpoint() -> HNil {
    HNil
}

/// This trait is used to extract data from a request.
#[async_trait(?Send)]
trait Extractable {
    type Output;
    async fn extract(&self, parts: &Parts, body: &Bytes) -> Self::Output;

    fn with_extractor<E: Extractor>(self, extractor: &E) -> HCons<&E, Self>
    where
        Self: Sized,
    {
        HCons {
            head: extractor,
            tail: self,
        }
    }
}

#[async_trait(?Send)]
impl<E: Extractor> Extractable for E {
    type Output = E::Output;

    async fn extract(&self, request: &Parts, body: &Bytes) -> Self::Output {
        self.extract(request, body).await
    }
}

#[async_trait(?Send)]
impl Extractable for HNil {
    /// The output of extracting from an empty HList is an empty HList.
    type Output = HNil;

    async fn extract(&self, _: &Parts, _: &Bytes) -> Self::Output {
        HNil
    }
}

#[async_trait(?Send)]
impl<E: Extractor, R: Extractable> Extractable for HCons<&E, R> {
    /// The output of extracting from an HList with a head and a tail is the
    /// output of extracting from the head and the output of extracting from
    /// the tail.
    type Output = HCons<E::Output, R::Output>;

    async fn extract(&self, request: &Parts, body: &Bytes) -> Self::Output {
        let head: <E as Extractor>::Output = self.head.extract(request, body).await;
        let tail: R::Output = self.tail.extract(request, body).await;
        HCons { head, tail }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Contact {
    name: String,
    email: String,
    age: u8,
}

#[tokio::main]
async fn main() {
    let endpoint = empty_endpoint();

    let contact = Contact {
        name: "John Doe".to_string(),
        email: "foo@john.com".to_string(),
        age: 42,
    };

    let contact_as_json = serde_json::to_string(&contact).unwrap();

    let request: Request<Body> = Request::builder()
        .uri("/hello/1337")
        .body(Body::from(contact_as_json))
        .unwrap();

    let (parts, body) = request.into_parts();

    let extract_first_part: PathExtractor<String> =
        PathExtractor(|request| request.uri.path().split("/").nth(1).unwrap().to_string());

    let extract_second_part: PathExtractor<u64> = PathExtractor(|request| {
        request
            .uri
            .path()
            .split("/")
            .nth(2)
            .unwrap()
            .parse::<u64>()
            .unwrap()
    });

    let extract_contact_from_body: BodyExtractor<Contact> =
        BodyExtractor(|body| serde_json::from_slice(body).unwrap());

    let endpoint2 = endpoint
        .with_extractor(&extract_first_part)
        .with_extractor(&extract_second_part)
        .with_extractor(&extract_contact_from_body);

    let bytes: Bytes = hyper::body::to_bytes(body).await.unwrap();

    let result: HCons<Contact, HCons<u64, HCons<String, HNil>>> =
        endpoint2.extract(&parts, &bytes).await;

    print!("{:?}", result);
}
