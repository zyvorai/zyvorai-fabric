// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/int64planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ resource.Resource = &inferenceDeploymentResource{}

type inferenceDeploymentResource struct {
	client *Client
}

type inferenceDeploymentModel struct {
	Name     types.String `tfsdk:"name"`
	Model    types.String `tfsdk:"model"`
	Profile  types.String `tfsdk:"profile"`
	Replicas types.Int64  `tfsdk:"replicas"`
	Tenant   types.String `tfsdk:"tenant"`
	Phase    types.String `tfsdk:"phase"`
	Message  types.String `tfsdk:"message"`
}

func NewInferenceDeploymentResource() resource.Resource {
	return &inferenceDeploymentResource{}
}

func (r *inferenceDeploymentResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_inference_deployment"
}

func (r *inferenceDeploymentResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Fabric AI InferenceDeployment (model + profile × replicas).",
		Attributes: map[string]schema.Attribute{
			"name": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"model": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"profile": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"replicas": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"tenant":  schema.StringAttribute{Optional: true},
			"phase":   schema.StringAttribute{Computed: true},
			"message": schema.StringAttribute{Computed: true},
		},
	}
}

func (r *inferenceDeploymentResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
	if req.ProviderData == nil {
		return
	}
	client, ok := req.ProviderData.(*Client)
	if !ok {
		resp.Diagnostics.AddError("Invalid provider data", "expected *Client")
		return
	}
	r.client = client
}

func (r *inferenceDeploymentResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan inferenceDeploymentModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	replicas := plan.Replicas.ValueInt64()
	if replicas == 0 {
		replicas = 1
	}
	creq := createInferenceDeploymentRequest{
		Name:     plan.Name.ValueString(),
		Model:    plan.Model.ValueString(),
		Profile:  plan.Profile.ValueString(),
		Replicas: replicas,
	}
	if !plan.Tenant.IsNull() && plan.Tenant.ValueString() != "" {
		v := plan.Tenant.ValueString()
		creq.Tenant = &v
	}
	d, err := r.client.CreateInferenceDeployment(ctx, creq)
	if err != nil {
		resp.Diagnostics.AddError("Create InferenceDeployment failed", err.Error())
		return
	}
	applyDeployment(d, &plan)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *inferenceDeploymentResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state inferenceDeploymentModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	d, err := r.client.GetInferenceDeployment(ctx, state.Name.ValueString())
	if err != nil {
		if IsNotFound(err) {
			resp.State.RemoveResource(ctx)
			return
		}
		resp.Diagnostics.AddError("Read failed", err.Error())
		return
	}
	applyDeployment(d, &state)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *inferenceDeploymentResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	var plan inferenceDeploymentModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	replicas := plan.Replicas.ValueInt64()
	if replicas == 0 {
		replicas = 1
	}
	d, err := r.client.ScaleInferenceDeployment(ctx, plan.Name.ValueString(), replicas)
	if err != nil {
		resp.Diagnostics.AddError("Scale InferenceDeployment failed", err.Error())
		return
	}
	applyDeployment(d, &plan)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *inferenceDeploymentResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state inferenceDeploymentModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteInferenceDeployment(ctx, state.Name.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete InferenceDeployment failed", err.Error())
	}
}

func (r *inferenceDeploymentResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("name"), req, resp)
}

func applyDeployment(d *inferenceDeploymentRecord, plan *inferenceDeploymentModel) {
	plan.Name = types.StringValue(d.Name)
	plan.Model = types.StringValue(d.Model)
	plan.Profile = types.StringValue(d.Profile)
	plan.Replicas = types.Int64Value(d.Replicas)
	plan.Phase = types.StringValue(d.Status.Phase)
	if d.Status.Message != "" {
		plan.Message = types.StringValue(d.Status.Message)
	} else {
		plan.Message = types.StringNull()
	}
}
