using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class PlayerColor : MonoBehaviour
{
    public Component ColorMark;
    public int a;
    public int b;

    // Use this for initialization
    void Start () {
        a = 1;
        b = 2;
        float h = (float)b / a;
        ColorMark.GetComponent<SpriteRenderer>().color = Color.HSVToRGB(h, 1, 1);
	}
}
