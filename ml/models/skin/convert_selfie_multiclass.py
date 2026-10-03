"""Convert MediaPipe's Selfie Multiclass segmenter (TFLite, Apache-2.0) to ONNX for aura-infer.

The model labels every pixel of a person photograph as one of six classes:
background, hair, body skin, face skin, clothes, other (accessories). It is the open
source answer to "which pixels are this person's skin" that AURA's sampled-colour
selection could only approximate. Source and licence: assets/models/selfie_multiclass/.

Why a hand-written converter: tf2onnx needs TensorFlow (not available for the Python on
the build machines) and tflite2onnx does not support SUM. The graph is small (175 ops of
eleven kinds), so this script reads the flatbuffer directly and emits an opset-13 graph
that uses only operators aura-infer implements.

Layout: TFLite tensors are NHWC. Spatial operators (Conv, ConvTranspose, Resize) run in
NCHW here, and the attention blocks (Reshape, Transpose, Softmax, ReduceSum) keep their
original NHWC semantics. Every tensor carries which layout it is stored in and a
Transpose is inserted only where an operator needs the other one, so the result computes
exactly the TFLite graph.

Usage:
    python ml/models/skin/convert_selfie_multiclass.py IN.tflite OUT.onnx [--check IMAGE]

Requires the `tflite` and `onnx` packages (and `onnxruntime` + Pillow for --check).
"""
import argparse
import sys

import numpy as np
import onnx
import tflite
from onnx import TensorProto, helper, numpy_helper

OPS = {v: k for k, v in tflite.BuiltinOperator.__dict__.items() if not k.startswith('_')}
SAME, VALID = 0, 1
RELU, RELU6 = 1, 3


class Converter:
    def __init__(self, path):
        self.buf = open(path, 'rb').read()
        self.model = tflite.Model.GetRootAsModel(self.buf, 0)
        self.graph = self.model.Subgraphs(0)
        self.nodes = []
        self.inits = []
        # tflite tensor index -> (onnx name, layout); layout is 'raw' (as TFLite) or 'nchw'.
        self.values = {}
        self.converted = {}
        self.counter = 0

    # -- helpers -----------------------------------------------------------------------
    def name(self, hint):
        self.counter += 1
        return f'{hint}_{self.counter}'

    def shape(self, t):
        tensor = self.graph.Tensors(t)
        return [int(v) for v in tensor.ShapeAsNumpy()] if tensor.ShapeLength() else []

    def const(self, t):
        tensor = self.graph.Tensors(t)
        buffer = self.model.Buffers(tensor.Buffer())
        if buffer.DataLength() == 0:
            return None
        dtype = {0: np.float32, 2: np.int32, 4: np.int64}[tensor.Type()]
        data = np.frombuffer(buffer.DataAsNumpy().tobytes(), dtype=dtype)
        return data.reshape(self.shape(t)) if self.shape(t) else data.reshape(())

    def init(self, array, hint):
        name = self.name(hint)
        self.inits.append(numpy_helper.from_array(np.ascontiguousarray(array), name))
        return name

    def node(self, op, inputs, hint, **attrs):
        out = self.name(hint)
        self.nodes.append(helper.make_node(op, inputs, [out], **attrs))
        return out

    def get(self, t, want):
        """The ONNX name of TFLite tensor `t`, stored in layout `want`."""
        name, layout = self.values[t]
        if layout == want or len(self.shape(t)) != 4:
            return name
        key = (t, want)
        if key not in self.converted:
            perm = [0, 3, 1, 2] if want == 'nchw' else [0, 2, 3, 1]
            self.converted[key] = self.node('Transpose', [name], 'to_' + want, perm=perm)
        return self.converted[key]

    def activation(self, name, act):
        if act == 0:
            return name
        if act == RELU:
            return self.node('Relu', [name], 'relu')
        if act == RELU6:
            lo = self.init(np.array(0, np.float32), 'lo')
            hi = self.init(np.array(6, np.float32), 'hi')
            return self.node('Clip', [name, lo, hi], 'relu6')
        raise ValueError(f'activation {act}')

    def pads(self, padding, in_hw, k_hw, stride, dil=(1, 1)):
        if padding == VALID:
            return [0, 0, 0, 0]
        out = []
        for size, k, s, d in zip(in_hw, k_hw, stride, dil):
            effective = (k - 1) * d + 1
            total = max((-(-size // s) - 1) * s + effective - size, 0)
            out.append((total // 2, total - total // 2))
        (t, b), (l, r) = out
        return [t, l, b, r]

    # -- operators ---------------------------------------------------------------------
    def convert(self):
        g = self.graph
        input_t = g.Inputs(0)
        _, h, w, c = self.shape(input_t)
        self.values[input_t] = ('image', 'nchw')
        inputs = [helper.make_tensor_value_info('image', TensorProto.FLOAT, [1, c, h, w])]
        for i in range(g.OperatorsLength()):
            op = g.Operators(i)
            code = self.model.OperatorCodes(op.OpcodeIndex())
            kind = OPS[max(code.BuiltinCode(), code.DeprecatedBuiltinCode())]
            ins = [op.Inputs(j) for j in range(op.InputsLength())]
            out_t = op.Outputs(0)
            options = op.BuiltinOptions()
            result = getattr(self, 'op_' + kind.lower())(ins, out_t, options)
            self.values[out_t] = result
        out_t = g.Outputs(0)
        _, oh, ow, oc = self.shape(out_t)
        final = self.get(out_t, 'nchw')
        # Rename the producing node's output rather than adding an Identity node.
        for node in self.nodes:
            node.output[:] = ['logits' if o == final else o for o in node.output]
            node.input[:] = ['logits' if i == final else i for i in node.input]
        outputs = [helper.make_tensor_value_info('logits', TensorProto.FLOAT, [1, oc, oh, ow])]
        graph = helper.make_graph(self.nodes, 'selfie_multiclass_256x256', inputs, outputs, self.inits)
        model = helper.make_model(graph, opset_imports=[helper.make_opsetid('', 13)],
                                  producer_name='aura convert_selfie_multiclass.py')
        model.ir_version = 7
        onnx.checker.check_model(model)
        return model

    def op_conv_2d(self, ins, out_t, options):
        o = tflite.Conv2DOptions()
        o.Init(options.Bytes, options.Pos)
        x = self.get(ins[0], 'nchw')
        weight = self.const(ins[1])  # [O, kh, kw, I]
        _, ih, iw, _ = self.shape(ins[0])
        stride = (o.StrideH(), o.StrideW())
        dil = (o.DilationHFactor(), o.DilationWFactor())
        pads = self.pads(o.Padding(), (ih, iw), weight.shape[1:3], stride, dil)
        args = [x, self.init(weight.transpose(0, 3, 1, 2), 'w')]
        if len(ins) > 2 and ins[2] >= 0:
            args.append(self.init(self.const(ins[2]), 'b'))
        y = self.node('Conv', args, 'conv', strides=list(stride), dilations=list(dil), pads=pads,
                      kernel_shape=list(weight.shape[1:3]), group=1)
        return self.activation(y, o.FusedActivationFunction()), 'nchw'

    def op_depthwise_conv_2d(self, ins, out_t, options):
        o = tflite.DepthwiseConv2DOptions()
        o.Init(options.Bytes, options.Pos)
        assert o.DepthMultiplier() == 1
        x = self.get(ins[0], 'nchw')
        weight = self.const(ins[1])  # [1, kh, kw, C]
        _, ih, iw, channels = self.shape(ins[0])
        stride = (o.StrideH(), o.StrideW())
        dil = (o.DilationHFactor(), o.DilationWFactor())
        pads = self.pads(o.Padding(), (ih, iw), weight.shape[1:3], stride, dil)
        args = [x, self.init(weight.transpose(3, 0, 1, 2), 'dw'),
                self.init(self.const(ins[2]), 'b')]
        y = self.node('Conv', args, 'dwconv', strides=list(stride), dilations=list(dil),
                      pads=pads, kernel_shape=list(weight.shape[1:3]), group=channels)
        return self.activation(y, o.FusedActivationFunction()), 'nchw'

    def op_transpose_conv(self, ins, out_t, options):
        o = tflite.TransposeConvOptions()
        o.Init(options.Bytes, options.Pos)
        weight = self.const(ins[1])  # [O, kh, kw, I]
        x = self.get(ins[2], 'nchw')
        _, ih, iw, _ = self.shape(ins[2])
        _, oh, ow, _ = self.shape(out_t)
        stride = (o.StrideH(), o.StrideW())
        kh, kw = weight.shape[1:3]
        total = [(ih - 1) * stride[0] + kh - oh, (iw - 1) * stride[1] + kw - ow]
        assert total == [0, 0], total  # the one instance in this model has no padding
        args = [x, self.init(weight.transpose(3, 0, 1, 2), 'tw')]
        if len(ins) > 3 and ins[3] >= 0:
            args.append(self.init(self.const(ins[3]), 'b'))
        y = self.node('ConvTranspose', args, 'deconv', strides=list(stride),
                      kernel_shape=[kh, kw], pads=[0, 0, 0, 0])
        return self.activation(y, o.FusedActivationFunction()), 'nchw'

    def binary(self, kind, ins, options_cls, options):
        o = options_cls()
        o.Init(options.Bytes, options.Pos)
        consts = [self.const(t) for t in ins]
        shapes = [self.shape(t) for t in ins]
        if consts[0] is None and consts[1] is None:
            four = all(len(s) == 4 for s in shapes)
            layout = 'nchw' if four and any(self.values[t][1] == 'nchw' for t in ins) else 'raw'
            args = [self.get(t, layout) for t in ins]
        else:
            k = 0 if consts[0] is not None else 1
            act_t = ins[1 - k]
            layout = self.values[act_t][1] if len(shapes[1 - k]) == 4 else 'raw'
            value = consts[k].astype(np.float32)
            if layout == 'nchw' and value.ndim > 0:
                value = value.reshape((1,) * (4 - value.ndim) + value.shape).transpose(0, 3, 1, 2)
            args = [None, None]
            args[k] = self.init(value, 'k')
            args[1 - k] = self.get(act_t, layout)
        y = self.node(kind, args, kind.lower())
        return self.activation(y, o.FusedActivationFunction()), layout

    def op_add(self, ins, out_t, options):
        return self.binary('Add', ins, tflite.AddOptions, options)

    def op_mul(self, ins, out_t, options):
        return self.binary('Mul', ins, tflite.MulOptions, options)

    def op_reshape(self, ins, out_t, options):
        x = self.get(ins[0], 'raw')
        shape = np.array(self.shape(out_t), np.int64)
        return self.node('Reshape', [x, self.init(shape, 'shape')], 'reshape'), 'raw'

    def op_transpose(self, ins, out_t, options):
        x = self.get(ins[0], 'raw')
        perm = [int(v) for v in self.const(ins[1])]
        return self.node('Transpose', [x], 'transpose', perm=perm), 'raw'

    def op_softmax(self, ins, out_t, options):
        o = tflite.SoftmaxOptions()
        o.Init(options.Bytes, options.Pos)
        assert o.Beta() == 1.0
        x = self.get(ins[0], 'raw')
        return self.node('Softmax', [x], 'softmax', axis=-1), 'raw'

    def op_sum(self, ins, out_t, options):
        o = tflite.ReducerOptions()
        o.Init(options.Bytes, options.Pos)
        x = self.get(ins[0], 'raw')
        axes = np.atleast_1d(self.const(ins[1])).astype(np.int64)
        return self.node('ReduceSum', [x, self.init(axes, 'axes')], 'sum',
                         keepdims=int(o.KeepDims())), 'raw'

    def resize(self, ins, out_t, options, mode):
        x = self.get(ins[0], 'nchw')
        _, ih, iw, _ = self.shape(ins[0])
        _, oh, ow, _ = self.shape(out_t)
        scales = np.array([1, 1, oh / ih, ow / iw], np.float32)
        roi = self.init(np.zeros(0, np.float32), 'roi')
        if mode == 'linear':
            attrs = dict(mode='linear', coordinate_transformation_mode='half_pixel')
        else:
            # TF half-pixel nearest at an integer factor equals floor(x / factor).
            assert oh % ih == 0 and ow % iw == 0
            attrs = dict(mode='nearest', coordinate_transformation_mode='asymmetric',
                         nearest_mode='floor')
        return self.node('Resize', [x, roi, self.init(scales, 'scales')], 'resize', **attrs), 'nchw'

    def op_resize_bilinear(self, ins, out_t, options):
        o = tflite.ResizeBilinearOptions()
        o.Init(options.Bytes, options.Pos)
        assert o.HalfPixelCenters() and not o.AlignCorners()
        return self.resize(ins, out_t, options, 'linear')

    def op_resize_nearest_neighbor(self, ins, out_t, options):
        o = tflite.ResizeNearestNeighborOptions()
        o.Init(options.Bytes, options.Pos)
        assert not o.AlignCorners()
        return self.resize(ins, out_t, options, 'nearest')


CLASSES = ['background', 'hair', 'body-skin', 'face-skin', 'clothes', 'others']


def check(path, image_path):
    """Run the converted model with onnxruntime and print per-class coverage."""
    import onnxruntime as ort
    from PIL import Image
    image = Image.open(image_path).convert('RGB').resize((256, 256), Image.BILINEAR)
    x = (np.asarray(image, np.float32) - 127.5) / 127.5
    x = x.transpose(2, 0, 1)[None]
    session = ort.InferenceSession(path, providers=['CPUExecutionProvider'])
    logits = session.run(None, {'image': x})[0][0]
    labels = logits.argmax(0)
    for k, label in enumerate(CLASSES):
        print(f'{label:>10}: {np.mean(labels == k):6.1%}')
    return logits


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('tflite')
    parser.add_argument('onnx')
    parser.add_argument('--check')
    args = parser.parse_args()
    model = Converter(args.tflite).convert()
    onnx.save(model, args.onnx)
    ops = sorted({n.op_type for n in model.graph.node})
    print(f'wrote {args.onnx}: {len(model.graph.node)} nodes, ops {ops}')
    if args.check:
        check(args.onnx, args.check)
    return 0


if __name__ == '__main__':
    sys.exit(main())
