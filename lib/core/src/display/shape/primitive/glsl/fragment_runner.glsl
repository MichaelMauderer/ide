/// This code is the body of the fragment shader main function of a GLSL shape.

Env   env      = Env(1);
vec2  position = input_local.xy ;
Shape shape    = run(env,position);
float alpha  = shape.color.color.raw.a;

if (input_display_mode == 0) {
    output_color = srgba(unpremultiply(shape.color)).raw;
    output_color.rgb *= alpha;
} else if (input_display_mode == 1) {
    Rgb col = distance_meter(shape.sdf.distance, 200.0 * input_zoom * input_pixel_ratio, 200.0/input_zoom * input_pixel_ratio);
    output_color = rgba(col).raw;
}

uint alpha_no_aa = alpha > 0.5 ? uint(1) : uint(0);

output_id = uvec4(input_symbol_id,input_instance_id,0,alpha_no_aa);
output_id.r *= alpha_no_aa;
output_id.g *= alpha_no_aa;
output_id.b *= alpha_no_aa;
