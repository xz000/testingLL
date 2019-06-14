using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WeiQi : MonoBehaviour
{
    private int[,] Chou = new int[19, 19];
    private int[,] Lon = new int[19, 19];
    bool nowBlack = false;
    public GameObject qizi;
    public static Color PanColor;
    // Start is called before the first frame update
    void Start()
    {
        PanColor = GetComponent<SpriteRenderer>().color;
        chushihuaQi();
    }

    void Update()
    {
        if (Input.GetMouseButtonDown(0))
            CatchHim();
    }

    public void setChou(WeiQiZi qizi, int color)
    {
        Chou[qizi.x + 9, qizi.y + 9] = color;
        switch (color)
        {
            case 1:
                qizi.goBlack();
                break;
            case 2:
                qizi.goWhite();
                break;
            case 0:
                qizi.goDie();
                break;
        }
    }

    private void jieLon()
    {
        Lon = new int[19, 19];
        int n = 0;
        for (int i = 0; i < 19; i++)
        {
            for (int j = 0; j < 19; j++)
            {
                if (Lon[i, j] != 0)
                    continue;
                if (Chou[i, j] != 0)
                {
                    n++;
                    lianZhu(i, j, n);
                }
            }
        }
    }

    private void lianZhu(int i, int j, int n)
    {
        Lon[i, j] = n;
        Debug.Log(i + "," + j + "," + n);
        for (int a = -1; a < 2; a += 2)
        {
            if (19 > i + a && i + a >= 0)
                if (Chou[i + a, j] == Chou[i, j] && Lon[i + a, j] == 0)
                    lianZhu(i + a, j, n);
            if (19 > j + a && j + a >= 0)
                if (Chou[i, j + a] == Chou[i, j] && Lon[i, j + a] == 0)
                    lianZhu(i, j + a, n);
        }
    }

    void chushihuaQi()
    {
        for (int i = -9; i < 10; i++)
        {
            for (int j = -9; j < 10; j++)
            {
                GameObject nq = GameObject.Instantiate(qizi, new Vector3(i, j, 0), Quaternion.identity);
                nq.GetComponent<WeiQiZi>().setXY(i, j);
            }
        }
    }

    public void ChangeColor()
    {
        nowBlack = !nowBlack;
    }

    public void ToBlack()
    {
        nowBlack = true;
    }

    public void ToWhite()
    {
        nowBlack = false;
    }

    public void eat1B()
    {

    }

    public void eat1W()
    {

    }

    void CatchHim()
    {
        Collider2D hit = Physics2D.OverlapPoint(Camera.main.ScreenToWorldPoint(Input.mousePosition));
        if (!hit)
            return;
        if (hit.GetComponent<WeiQiZi>())
        {
            if (hit.GetComponent<WeiQiZi>().srZhong.color != WeiQi.PanColor)
                return;
            if (nowBlack)
                setChou(hit.GetComponent<WeiQiZi>(), 1);
            else
                setChou(hit.GetComponent<WeiQiZi>(), 2);
            jieLon();
        }
        if (hit.GetComponent<WeiQiControl>())
        {
            hit.GetComponent<WeiQiControl>().pickMe();
        }
    }

    void Panduan()
    {

    }
}