use crate::endpoints::{
    context_exclusion::ContextExclusionEndpointDef,
    events::EventsEndpointDef,
    graph::{
        endpoint_flow::EndpointFlowEndpointDef,
        graph_endpoints::JavaEndpointsEndpointDef,
        graph_initial::GraphInitialEndpointDef,
        graph_neighbors::GraphNeighborsEndpointDef,
        graph_search::GraphSearchEndpointDef,
        graph_stats::GraphStatsEndpointDef,
        service_calls::JavaServiceCallsEndpointDef,
    },
    info::InfoEndpointDef,
    workspace_delete::WorkspaceDeleteEndpointDef,
    workspace_index::WorkspaceIndexEndpointDef,
    workspace_list::WorkspaceListEndpointDef,
};
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
#[derive(Default)]
pub struct ApiContract {
    pub info: InfoEndpointDef,
    pub workspace_index: WorkspaceIndexEndpointDef,
    pub workspace_list: WorkspaceListEndpointDef,
    pub workspace_delete: WorkspaceDeleteEndpointDef,
    pub index: WorkspaceIndexEndpointDef,
    pub events: EventsEndpointDef,
    pub graph_initial: GraphInitialEndpointDef,
    pub graph_neighbors: GraphNeighborsEndpointDef,
    pub graph_search: GraphSearchEndpointDef,
    pub graph_stats: GraphStatsEndpointDef,
    pub context_exclusion: ContextExclusionEndpointDef,
    pub java_endpoints: JavaEndpointsEndpointDef,
    pub java_service_calls: JavaServiceCallsEndpointDef,
    pub endpoint_flow: EndpointFlowEndpointDef,
}
