using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WeiQiZi : MonoBehaviour
{
    public SpriteRenderer srDa;
    public SpriteRenderer srZhong;
    public SpriteRenderer srXiao;
    public int x;
    public int y;

    public void setXY(int a, int b)
    {
        x = a;
        y = b;
        srDa.color = srZhong.color = srXiao.color = WeiQi.PanColor;
    }

    public void goWhite()
    {
        srZhong.color = Color.white;
    }

    public void goBlack()
    {
        srZhong.color = Color.black;
    }

    public void goDie()
    {
        srZhong.color = WeiQi.PanColor;
    }

    private void OnMouseEnter()
    {
        if (srZhong.color == WeiQi.PanColor)
            srDa.color = Color.red;
    }

    private void OnMouseExit()
    {
        srDa.color = WeiQi.PanColor;
    }
}