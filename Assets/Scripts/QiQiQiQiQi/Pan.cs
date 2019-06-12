using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class Pan : MonoBehaviour
{
    public Material mat;
    Vector3 tempPos;
    Vector3 drawPos;

    void OnPostRender()
    {
        GL.PushMatrix();
        mat.SetPass(0);
        GL.LoadOrtho();
        GL.Begin(GL.LINES);
        GL.Color(Color.gray);
        for (int i = -9; i < 10; i++)
        {
            tempPos = Camera.main.WorldToScreenPoint(new Vector3(i, 9, 0));
            drawPos = new Vector3(tempPos.x / Screen.width, tempPos.y / Screen.height, 0);
            GL.Vertex(drawPos);
            tempPos = Camera.main.WorldToScreenPoint(new Vector3(i, -9, 0));
            drawPos = new Vector3(tempPos.x / Screen.width, tempPos.y / Screen.height, 0);
            GL.Vertex(drawPos);
        }
        for (int i = -9; i < 10; i++)
        {
            tempPos = Camera.main.WorldToScreenPoint(new Vector3(9, i, 0));
            drawPos = new Vector3(tempPos.x / Screen.width, tempPos.y / Screen.height, 0);
            GL.Vertex(drawPos);
            tempPos = Camera.main.WorldToScreenPoint(new Vector3(-9, i, 0));
            drawPos = new Vector3(tempPos.x / Screen.width, tempPos.y / Screen.height, 0);
            GL.Vertex(drawPos);
        }
        GL.End();
        GL.PopMatrix();
    }
}