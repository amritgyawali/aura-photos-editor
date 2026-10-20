import { expect, it } from 'vitest';
import { eyeRotation, faceFrame, pixelCrop, printPixels, probePixel, ratioCrop, subjectRotation, type FramingContext, type FramingFace } from './framingMath';
import type { RenderDto } from '../../ipc/types';
const face: FramingFace = { bounds: [.4,.2,.6,.4], eyes: [[.44,.28],[.56,.3]], eyesMeasured: 2, confidence: .9 };
const context: FramingContext = { width: 6000, height: 4000, recipeHash: 'h', faces: [face], subjectBounds: [.2,.1,.8,.9], subjectAxis: 90, reasons: [] };
it('fits a requested ratio around an off-centre anchor without leaving the frame', () => {
  const c = ratioCrop(6000,4000,4/5,[.85,.3]);
  expect((c[2]-c[0])*6000/((c[3]-c[1])*4000)).toBeCloseTo(4/5);
  expect(c[2]).toBe(1); expect(c[1]).toBe(0);
  expect(() => ratioCrop(10,10,NaN)).toThrow();
});
it('uses original pixels for exact crops and refuses upscaling or fractional dimensions', () => {
  const c = pixelCrop(6000,4000,[1200,1500]);
  expect((c[2]-c[0])*6000).toBeCloseTo(1200); expect((c[3]-c[1])*4000).toBeCloseTo(1500);
  expect(() => pixelCrop(100,100,[120,20])).toThrow(/fit inside/); expect(() => pixelCrop(100,100,[12.5,20])).toThrow();
});
it('converts millimetres and PPI to delivery pixels with bounded dimensions', () => {
  expect(printPixels([25.4,50.8],300)).toEqual([300,600]); expect(printPixels([35,45],300)).toEqual([413,531]);
  expect(() => printPixels([0,45],300)).toThrow(); expect(() => printPixels([35,45],1300)).toThrow();
});
it('places the measured face using framing rules and refuses a ratio that would cut it', () => {
  const crop = faceFrame(context, face, .8, .5, .1, .5);
  expect(crop[0]).toBeLessThanOrEqual(face.bounds[0]); expect(crop[3]).toBeGreaterThanOrEqual(face.bounds[3]);
  expect((crop[2]-crop[0])*6000/((crop[3]-crop[1])*4000)).toBeCloseTo(.8);
  expect(() => faceFrame({ ...context, width: 1000, height: 1000 }, { ...face,bounds:[.2,.1,.8,.9] },2,.5,.1,.5)).toThrow(/cut/);
});
it('uses pixel aspect for eye alignment and refuses estimated eyes and ambiguous axes', () => {
  expect(eyeRotation(face,6000,4000)).toBeCloseTo(-Math.atan2(80,720)*180/Math.PI);
  expect(() => eyeRotation({ ...face, eyesMeasured: 1 },6000,4000)).toThrow(/Both eyes/);
  expect(subjectRotation(80)).toBe(10); expect(subjectRotation(-80)).toBe(-10); expect(() => subjectRotation(20)).toThrow();
});
it('reads a displayed RGB pixel and D65 Lab without changing the preview', () => {
  const frame = { width:3,height:1,rgbBase64:btoa(String.fromCharCode(255,255,255,0,0,0,255,0,0)) } as RenderDto;
  const white = probePixel(frame,[0,0]); expect(white.lab[0]).toBeCloseTo(100,3); expect(white.lab[1]).toBeCloseTo(0,2);
  expect(probePixel(frame,[.5,0]).lab).toEqual([0,0,0]);
  const red = probePixel(frame,[1,1]); expect(red.rgb).toEqual([255,0,0]); expect(red.lab[0]).toBeCloseTo(53.24,1);
  expect(frame.rgbBase64).toBe(btoa(String.fromCharCode(255,255,255,0,0,0,255,0,0))); expect(() => probePixel(frame,[-1,0])).toThrow();
});
