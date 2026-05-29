use crate::{
    contracts::{
        IndexUpsertRequest, IndexUpsertResponse, SearchHit, SearchRecord, SearchRequest,
        SearchResponse,
    },
    errors::SearchError,
    traits::{DocumentStore, Embedder, IndexedPoint, VectorIndex},
};

#[derive(Debug, Clone)]
pub struct SearchService<E, I, D> {
    pub embedder: E,
    pub index: I,
    pub documents: D,
}

impl<E, I, D> SearchService<E, I, D> {
    pub fn new(embedder: E, index: I, documents: D) -> Self {
        Self {
            embedder,
            index,
            documents,
        }
    }
}

impl
    SearchService<
        crate::memory::MemoryEmbedder,
        crate::memory::MemoryVectorIndex,
        crate::memory::MemoryDocumentStore,
    >
{
    pub fn in_memory() -> Self {
        Self::new(
            crate::memory::MemoryEmbedder,
            crate::memory::MemoryVectorIndex::default(),
            crate::memory::MemoryDocumentStore::default(),
        )
    }
}

impl<E, I, D> SearchService<E, I, D>
where
    E: Embedder,
    I: VectorIndex,
    D: DocumentStore,
{
    pub async fn health(&self) -> Result<(), SearchError> {
        Ok(())
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse, SearchError> {
        let query = request.query.trim();
        if query.is_empty() {
            return Err(SearchError::EmptyQuery);
        }
        if request.top_k == 0 {
            return Err(SearchError::InvalidTopK);
        }

        let mut filters = request.filters.unwrap_or_default();
        if let Some(lang) = request.lang {
            filters
                .metadata
                .insert("lang".to_string(), serde_json::Value::String(lang));
        }
        if let Some(market) = request.market {
            filters
                .metadata
                .insert("market".to_string(), serde_json::Value::String(market));
        }

        let vector = self
            .embedder
            .embed_query(query)
            .await
            .map_err(|err| SearchError::Embedder(err.to_string()))?;

        let ranked = self
            .index
            .search(&vector, request.top_k, &filters)
            .await
            .map_err(|err| SearchError::Index(err.to_string()))?;

        let mut by_id = std::collections::HashMap::new();
        let missing_ids: Vec<String> = ranked
            .iter()
            .filter(|hit| hit.record.is_none())
            .map(|hit| hit.id.clone())
            .collect();
        if !missing_ids.is_empty() {
            let hydrated = self
                .documents
                .fetch_documents(&missing_ids)
                .await
                .map_err(|err| SearchError::Store(err.to_string()))?;
            for doc in hydrated {
                by_id.insert(doc.id.clone(), doc);
            }
        }

        let hits = ranked
            .into_iter()
            .filter_map(|mut ranked_hit| {
                let record = ranked_hit
                    .record
                    .take()
                    .or_else(|| by_id.remove(&ranked_hit.id))?;
                Some((ranked_hit, record))
            })
            .map(|(ranked_hit, record)| SearchHit {
                id: record.id.clone(),
                text: record.text.clone(),
                score: ranked_hit.score,
                source: record.source.clone(),
                metadata: record.metadata.clone(),
            })
            .collect();

        Ok(SearchResponse { hits })
    }

    pub async fn upsert(
        &self,
        request: IndexUpsertRequest,
    ) -> Result<IndexUpsertResponse, SearchError> {
        if request.reset {
            self.index
                .reset()
                .await
                .map_err(|err| SearchError::Index(err.to_string()))?;
            self.documents
                .reset()
                .await
                .map_err(|err| SearchError::Store(err.to_string()))?;
        }

        let records: Vec<SearchRecord> = request.documents.iter().map(SearchRecord::from).collect();
        let indexed_docs = self
            .documents
            .upsert_documents(records.clone())
            .await
            .map_err(|err| SearchError::Store(err.to_string()))?;

        let texts: Vec<String> = records.iter().map(|record| record.text.clone()).collect();
        let vectors = self
            .embedder
            .embed_documents(&texts)
            .await
            .map_err(|err| SearchError::Embedder(err.to_string()))?;
        if vectors.len() != records.len() {
            return Err(SearchError::Embedder(format!(
                "embedder returned {} vectors for {} documents",
                vectors.len(),
                records.len()
            )));
        }

        let points: Vec<IndexedPoint> = records
            .iter()
            .zip(vectors)
            .map(|(record, vector)| {
                let mut point = IndexedPoint::from(record);
                point.vector = vector;
                point
            })
            .collect();

        self.index
            .upsert(points)
            .await
            .map_err(|err| SearchError::Index(err.to_string()))?;

        Ok(IndexUpsertResponse {
            indexed: indexed_docs,
        })
    }
}
